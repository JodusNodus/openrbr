//! A Wine prefix's `user.reg`: the DirectInput instance GUIDs dinput gave the joysticks (`save_registry_instances`),
//! each under its vendor and product ids and its index among the devices with them:
//! `[System\\CurrentControlSet\\Control\\MediaProperties\\PrivateProperties\\DirectInput\\VID_046D&PID_C266\\Calibration\\0]`
//! with `"GUID"=hex:..`. An RSF profile's `.acm` names its devices by these GUIDs.

/// A device dinput gave an instance GUID: its vendor and product ids and its index among devices with them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instance {
    pub guid: [u8; 16],
    pub vendor: u16,
    pub product: u16,
    pub index: u32,
}

/// The instances in a `user.reg`.
pub fn instances(user_reg: &str) -> Vec<Instance> {
    let mut out = Vec::new();
    let mut key = None;
    for line in user_reg.lines() {
        if line.starts_with('[') {
            key = line.split_once("\\\\DirectInput\\\\VID_").and_then(|(_, rest)| {
                let (ids, index) = rest.split_once("\\\\Calibration\\\\")?;
                let (vendor, product) = ids.split_once("&PID_")?;
                let index = index.split(']').next()?.parse().ok()?;
                Some((u16::from_str_radix(vendor, 16).ok()?, u16::from_str_radix(product, 16).ok()?, index))
            });
        } else if let (Some((vendor, product, index)), Some(hex)) = (key, line.strip_prefix("\"GUID\"=hex:")) {
            let bytes: Vec<u8> = hex.split(',').filter_map(|b| u8::from_str_radix(b.trim(), 16).ok()).collect();
            if let Ok(guid) = bytes.try_into() {
                out.push(Instance { guid, vendor, product, index });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_instances() {
        let reg = "[Software\\\\Wine] 1\n\
            [System\\\\CurrentControlSet\\\\Control\\\\MediaProperties\\\\PrivateProperties\\\\DirectInput\\\\VID_046D&PID_C266\\\\Calibration\\\\0] 1791574968\n\
            #time=1dc\n\
            \"GUID\"=hex:90,6c,30,9b,19,c4,f1,11,80,03,00,00,44,45,53,54\n\
            \"Joystick Id\"=hex:00,00,00,00\n";
        let guid = [0x90, 0x6c, 0x30, 0x9b, 0x19, 0xc4, 0xf1, 0x11, 0x80, 0x03, 0, 0, 0x44, 0x45, 0x53, 0x54];
        assert_eq!(instances(reg), [Instance { guid, vendor: 0x046d, product: 0xc266, index: 0 }]);
    }
}
