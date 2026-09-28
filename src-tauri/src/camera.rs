//! "Is any camera in use right now?" — one cheap, permission-free check per OS.

/// Windows records every app's camera session under CapabilityAccessManager.
/// A session with a start time but no stop time is live.
#[cfg(windows)]
pub fn in_use() -> bool {
    use winreg::{enums::*, RegKey};

    const STORE: &str =
        r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\webcam";

    fn any_live(key: &RegKey) -> bool {
        key.enum_keys().flatten().any(|name| {
            let Ok(app) = key.open_subkey(&name) else { return false };
            if name == "NonPackaged" {
                return any_live(&app);
            }
            let start: u64 = app.get_value("LastUsedTimeStart").unwrap_or(0);
            let stop: u64 = app.get_value("LastUsedTimeStop").unwrap_or(1);
            start > 0 && stop == 0
        })
    }

    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].into_iter().any(|hive| {
        RegKey::predef(hive)
            .open_subkey(STORE)
            .map(|k| any_live(&k))
            .unwrap_or(false)
    })
}

/// CoreMediaIO knows whether each video device is "running somewhere".
#[cfg(target_os = "macos")]
pub fn in_use() -> bool {
    use std::{ffi::c_void, ptr};

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreMediaIO", kind = "framework")]
    extern "C" {
        fn CMIOObjectGetPropertyDataSize(
            object: u32, address: *const Address, qualifier_size: u32,
            qualifier: *const c_void, data_size: *mut u32,
        ) -> i32;
        fn CMIOObjectGetPropertyData(
            object: u32, address: *const Address, qualifier_size: u32,
            qualifier: *const c_void, data_size: u32, data_used: *mut u32, data: *mut c_void,
        ) -> i32;
    }

    const fn fourcc(s: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*s)
    }
    const SYSTEM_OBJECT: u32 = 1;
    let global = fourcc(b"glob");
    let devices_addr = Address { selector: fourcc(b"dev#"), scope: global, element: 0 };
    let running_addr = Address { selector: fourcc(b"gone"), scope: global, element: 0 };

    unsafe {
        let mut size = 0u32;
        if CMIOObjectGetPropertyDataSize(SYSTEM_OBJECT, &devices_addr, 0, ptr::null(), &mut size) != 0 {
            return false;
        }
        let mut devices = vec![0u32; size as usize / 4];
        let mut used = 0u32;
        if CMIOObjectGetPropertyData(
            SYSTEM_OBJECT, &devices_addr, 0, ptr::null(), size, &mut used,
            devices.as_mut_ptr().cast(),
        ) != 0
        {
            return false;
        }
        devices.iter().any(|&dev| {
            let mut running = 0u32;
            let mut used = 0u32;
            CMIOObjectGetPropertyData(
                dev, &running_addr, 0, ptr::null(), 4, &mut used,
                (&mut running as *mut u32).cast(),
            ) == 0
                && running != 0
        })
    }
}

/// Any process (that we can see) holding a /dev/video* node open.
#[cfg(target_os = "linux")]
pub fn in_use() -> bool {
    let Ok(procs) = std::fs::read_dir("/proc") else { return false };
    procs.flatten().any(|p| {
        let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else { return false };
        fds.flatten().any(|fd| {
            std::fs::read_link(fd.path())
                .map(|t| t.to_string_lossy().starts_with("/dev/video"))
                .unwrap_or(false)
        })
    })
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn in_use() -> bool {
    false
}
