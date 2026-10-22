//! Read constant arguments and field-copy layouts from two fixed initializer sequences.
//! This is a bounded data decoder, not an x86 execution engine: branches, arithmetic and unknown instructions fail.

use super::{Result, Tyre, Words};
use crate::pe;
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};
use std::collections::BTreeMap;

fn instructions(image: &pe::Image, start: u32, end: u32) -> Result<Vec<Instruction>> {
    let bytes = image.read(start, (end - start) as usize).ok_or("NGP initializer outside image")?;
    let instructions: Vec<_> =
        Decoder::with_ip(32, bytes, u64::from(start), DecoderOptions::NONE).into_iter().collect();
    Ok(instructions)
}

fn immediate(i: &Instruction, operand: u32) -> Result<u32> {
    match i.op_kind(operand) {
        OpKind::Immediate32 => Ok(i.immediate32()),
        OpKind::Immediate8to32 => Ok(i.immediate8to32() as u32),
        OpKind::Immediate8 => Ok(u32::from(i.immediate8())),
        OpKind::Immediate16 => Ok(u32::from(i.immediate16())),
        _ => Err(format!("NGP nonconstant operand at {:#x}", i.ip())),
    }
}

fn field(i: &Instruction) -> Result<i32> {
    if i.memory_index() != Register::None {
        return Err("NGP indexed initializer operand".into());
    }
    Ok(i.memory_displacement32() as i32)
}

/// Stack argument offsets to object field offsets, decoded from straight-line setters.
fn setter(image: &pe::Image, at: u32) -> Result<(Vec<(u32, i32)>, i32)> {
    let mut registers = BTreeMap::new();
    let mut fields = Vec::new();
    for i in instructions(image, at, at + 0x200)? {
        match i.mnemonic() {
            Mnemonic::Ret => return Ok((fields, if i.op_count() == 0 { 0 } else { immediate(&i, 0)? as i32 })),
            Mnemonic::Mov | Mnemonic::Movss => {
                if i.op0_kind() == OpKind::Register
                    && i.op1_kind() == OpKind::Memory
                    && i.memory_base() == Register::EBP
                {
                    registers.insert(i.op0_register(), field(&i)? - 8);
                } else if i.op0_kind() == OpKind::Memory && i.memory_base() == Register::ECX {
                    fields.push((field(&i)? as u32, *registers.get(&i.op1_register()).ok_or("NGP setter register")?));
                } else if !(i.op0_register() == Register::EBP && i.op1_register() == Register::ESP) {
                    return Err(format!("unsupported NGP setter at {:#x}", i.ip()));
                }
            }
            Mnemonic::Push | Mnemonic::Pop => {}
            _ => return Err(format!("unsupported NGP setter at {:#x}", i.ip())),
        }
    }
    Err("NGP setter has no return".into())
}

fn constants(image: &pe::Image, ranges: &[(u32, u32)], base: Register) -> Result<Words> {
    let mut stack = BTreeMap::new();
    let mut sp = 0i32;
    let mut pointers = BTreeMap::from([(base, 0u32)]);
    let mut output = BTreeMap::new();
    let mut setters = BTreeMap::new();
    for &(start, end) in ranges {
        for i in instructions(image, start, end)? {
            match i.mnemonic() {
                Mnemonic::Push => {
                    sp -= 4;
                    stack.insert(sp, immediate(&i, 0)?);
                }
                Mnemonic::Sub if i.op0_register() == Register::ESP => sp -= immediate(&i, 1)? as i32,
                Mnemonic::Lea => {
                    let base = *pointers.get(&i.memory_base()).ok_or("NGP initializer pointer")?;
                    pointers.insert(i.op0_register(), base.wrapping_add(field(&i)? as u32));
                }
                Mnemonic::Mov => {
                    if i.op0_kind() == OpKind::Register {
                        let pointer = *pointers.get(&i.op1_register()).ok_or("NGP initializer register")?;
                        pointers.insert(i.op0_register(), pointer);
                    } else if i.memory_base() == Register::ESP {
                        stack.insert(sp + field(&i)?, immediate(&i, 1)?);
                    } else if i.memory_base() != Register::EBP {
                        return Err(format!("unsupported NGP store at {:#x}", i.ip()));
                    }
                }
                Mnemonic::Call if i.op0_kind() == OpKind::NearBranch32 => {
                    let target = i.near_branch32();
                    if let std::collections::btree_map::Entry::Vacant(entry) = setters.entry(target) {
                        entry.insert(setter(image, target)?);
                    }
                    let (fields, cleanup) = &setters[&target];
                    let object = *pointers.get(&Register::ECX).ok_or("NGP setter object")?;
                    for &(offset, argument) in fields {
                        output.insert(object + offset, *stack.get(&(sp + argument)).ok_or("NGP unset argument")?);
                    }
                    sp += cleanup;
                }
                Mnemonic::Pop | Mnemonic::Ret => {}
                _ => return Err(format!("unsupported NGP initializer at {:#x}", i.ip())),
            }
        }
    }
    Ok(output.into_iter().collect())
}

pub(super) fn stage(image: &pe::Image) -> Result<[Words; 2]> {
    let mut tables = Vec::new();
    for (start, end) in [(0x1004e41f, 0x100539dc), (0x100539dd, 0x10058f9a)] {
        let mut words = constants(image, &[(0x1004e3c2, 0x1004e414), (start, end)], Register::ESI)?;
        for (lo, hi) in [(0x1004e364, 0x1004e382), (0x1004e3a7, 0x1004e3b1)] {
            for i in instructions(image, lo, hi)? {
                if i.mnemonic() != Mnemonic::Mov || i.memory_base() != Register::ESI {
                    return Err("NGP stage default is not a constant store".into());
                }
                words.push((field(&i)? as u32, immediate(&i, 1)?));
            }
        }
        words.sort_unstable();
        if words.len() != 2740 {
            return Err("NGP stage table shape changed".into());
        }
        tables.push(words);
    }
    tables.try_into().map_err(|_| "NGP stage cases".into())
}

fn copy_fields(image: &pe::Image, at: u32) -> Result<Vec<(u32, u32)>> {
    let mut registers = BTreeMap::new();
    let mut output = Vec::new();
    for i in instructions(image, at, at + 0x500)? {
        match i.mnemonic() {
            Mnemonic::Ret => return Ok(output),
            Mnemonic::Mov
                if i.op0_kind() == OpKind::Register
                    && i.op1_kind() == OpKind::Memory
                    && i.memory_base() == Register::EDX =>
            {
                registers.insert(i.op0_register(), field(&i)? as u32);
            }
            Mnemonic::Mov if i.op0_kind() == OpKind::Memory && i.memory_base() == Register::ECX => {
                output.push((field(&i)? as u32, *registers.get(&i.op1_register()).ok_or("NGP copy register")?));
            }
            Mnemonic::Mov | Mnemonic::Push | Mnemonic::Pop => {}
            _ => return Err(format!("unsupported NGP field copy at {:#x}", i.ip())),
        }
    }
    Err("NGP copy has no return".into())
}

pub(super) fn tyres(image: &pe::Image) -> Result<[Tyre; 7]> {
    let block: BTreeMap<_, _> = constants(image, &[(0x1003f635, 0x10041122)], Register::EBX)?.into_iter().collect();
    let definition = copy_fields(image, 0x100b79d0)?;
    let wheel = copy_fields(image, 0x100b7e70)?;
    if definition.len() != 100 || wheel.len() != 3 {
        return Err("NGP tyre table shape changed".into());
    }
    Ok(std::array::from_fn(|k| {
        let copy = |fields: &[(u32, u32)], base| {
            fields
                .iter()
                .map(|&(dest, source)| (base + dest, *block.get(&(k as u32 * 0x6ac + source)).unwrap_or(&0)))
                .collect()
        };
        Tyre { definition: copy(&definition, 0x56c), wheel: copy(&wheel, 0x6b0) }
    }))
}
