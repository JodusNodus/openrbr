//! The tyre's field tables (`crate::object!`). The names are the step footprint's (`frame_types.rs`), which still reads
//! by its own offsets; B2 moves `load_frame` and `store_frame` onto this table.

use crate::math::vec::Vec4;

crate::object! {
    /// `CTyre::CScratch` (`this`, the wheel's `+0xf00`): the words the tyre step reads and writes, and its pointers. The
    /// size runs to the wheel's end; the bytes no field names are not owned.
    pub struct ScratchObj, size 0x520 {
        vtable @ 0x000: u32,
        def @ 0x010: u32,
        state @ 0x014: u32,
        k_vec @ 0x160: Vec4,
        contact_frame @ 0x170: [Vec4; 3],
        /// The `CTyreMatData` copy (`+0x1a0..+0x29c`, `material::WORDS`) and the words the step keeps between them
        /// (`+0x1e4`, `+0x1e8`, `+0x1f4`, `+0x1f8`, `+0x208`, `+0x218`, `+0x228`, `+0x258`, `+0x288`, `+0x298`).
        material @ 0x1a0: [u32; 64],
        scale_300 @ 0x300: f32,
        r304 @ 0x304: f32,
        f308 @ 0x308: f32,
        tyre_factor @ 0x30c: f32,
        f310 @ 0x310: f32,
        tyre_factor_2 @ 0x314: f32,
        f318 @ 0x318: f32,
        f31c @ 0x31c: f32,
        ratio_324 @ 0x324: f32,
        ratio_328 @ 0x328: f32,
        contact_p @ 0x32c: [f32; 7],
        force_sum @ 0x348: f32,
        nh @ 0x34c: f32,
        zero_350 @ 0x350: f32,
        force_t1 @ 0x354: f32,
        force_b @ 0x358: f32,
        force_a @ 0x35c: f32,
        speed @ 0x360: f32,
        speed_sq @ 0x364: f32,
        slip_v @ 0x368: [f32; 3],
        long_contact_vel @ 0x374: f32,
        /// Float bits.
        spin @ 0x378: u32,
        slip_ratio_term @ 0x37c: f32,
        f380 @ 0x380: f32,
        f384 @ 0x384: f32,
        f_e2 @ 0x388: f32,
        heat_flow_e3 @ 0x38c: f32,
        heat_flow_e4 @ 0x390: f32,
        f394 @ 0x394: f32,
        f398 @ 0x398: f32,
        f39c @ 0x39c: f32,
        f3a0 @ 0x3a0: f32,
        f3a4 @ 0x3a4: f32,
        /// Float bits.
        arg_c @ 0x3a8: u32,
        f3ac @ 0x3ac: f32,
        scale_3b0 @ 0x3b0: f32,
        blend @ 0x3b4: f32,
        f3b8 @ 0x3b8: f32,
        p3bc @ 0x3bc: f32,
        tyre_index @ 0x3c0: u32,
        heat_t @ 0x3c4: f32,
        heat_f2 @ 0x3c8: f32,
        f3cc @ 0x3cc: f32,
        f3d0 @ 0x3d0: f32,
        f3d4 @ 0x3d4: f32,
        /// Float bits.
        arg_d @ 0x3d8: u32,
        f3dc @ 0x3dc: f32,
        f3e0 @ 0x3e0: f32,
        f3e4 @ 0x3e4: f32,
        f3e8 @ 0x3e8: f32,
        f3ec @ 0x3ec: f32,
        gate @ 0x3f0: f32,
        f3f4 @ 0x3f4: f32,
        f3f8 @ 0x3f8: f32,
        f3fc @ 0x3fc: f32,
        f400 @ 0x400: f32,
        flow @ 0x404: f32,
        /// The contact record (its `+0x5ac` is the contact angle) and the wear table.
        record @ 0x410: u32,
        wear @ 0x414: u32,
        signal @ 0x420: [u32; 24],
        contact_flag @ 0x480: u32,
        tyre_index_copy @ 0x484: u32,
        copy_490 @ 0x490: Vec4,
        copy_4a0 @ 0x4a0: Vec4,
        copy_4b0 @ 0x4b0: u32,
        copy_4b4 @ 0x4b4: u32,
        copy_4b8 @ 0x4b8: u32,
        e3 @ 0x4bc: f32,
        x4c0 @ 0x4c0: f32,
        y4c4 @ 0x4c4: f32,
        pair @ 0x4c8: [u32; 3],
        heat_r @ 0x4e0: f32,
        heat_sum_ab @ 0x4e4: f32,
        ngp_4e8 @ 0x4e8: f32,
        heat_t_inv @ 0x4ec: f32,
        acc_504 @ 0x504: f32,
        acc_508 @ 0x508: f32,
        acc_50c @ 0x50c: f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{check_table, Object};

    #[test]
    fn table_fits() {
        check_table::<ScratchObj>();
        assert_eq!(ScratchObj::def.at, super::super::DEF);
        assert_eq!(ScratchObj::state.at, super::super::STATE);
        assert_eq!(ScratchObj::speed.at, super::super::SPEED);
        assert_eq!(ScratchObj::spin.at, super::super::SPIN);
        assert_eq!(ScratchObj::FIELDS.len(), 84);
    }
}
