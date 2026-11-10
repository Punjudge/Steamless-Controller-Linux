//! RetroArch controller profiles ("autoconfig") for the virtual pads, so
//! RetroArch knows which button is which without the person mapping them.
//!
//! Button and axis numbers follow how RetroArch numbers an evdev device:
//! buttons in key-code order, axes in ABS-code order without the hat (udev),
//! or with the hat as the last two axes (linuxraw / joydev).

use std::path::PathBuf;

const XBOX_UDEV: &str = r#"input_driver = "udev"
input_device = "Microsoft X-Box 360 pad"
input_device_display_name = "Steamless Controller (Xbox 360)"
input_vendor_id = "1118"
input_product_id = "654"
input_b_btn = "0"
input_a_btn = "1"
input_y_btn = "2"
input_x_btn = "3"
input_l_btn = "4"
input_r_btn = "5"
input_select_btn = "6"
input_start_btn = "7"
input_menu_toggle_btn = "8"
input_l3_btn = "9"
input_r3_btn = "10"
input_up_btn = "h0up"
input_down_btn = "h0down"
input_left_btn = "h0left"
input_right_btn = "h0right"
input_l2_axis = "+2"
input_r2_axis = "+5"
input_l_x_plus_axis = "+0"
input_l_x_minus_axis = "-0"
input_l_y_plus_axis = "+1"
input_l_y_minus_axis = "-1"
input_r_x_plus_axis = "+3"
input_r_x_minus_axis = "-3"
input_r_y_plus_axis = "+4"
input_r_y_minus_axis = "-4"
"#;

const XBOX_LINUXRAW: &str = r#"input_driver = "linuxraw"
input_device = "Microsoft X-Box 360 pad"
input_device_display_name = "Steamless Controller (Xbox 360)"
input_b_btn = "0"
input_a_btn = "1"
input_y_btn = "2"
input_x_btn = "3"
input_l_btn = "4"
input_r_btn = "5"
input_select_btn = "6"
input_start_btn = "7"
input_menu_toggle_btn = "8"
input_l3_btn = "9"
input_r3_btn = "10"
input_up_axis = "-7"
input_down_axis = "+7"
input_left_axis = "-6"
input_right_axis = "+6"
input_l2_axis = "+2"
input_r2_axis = "+5"
input_l_x_plus_axis = "+0"
input_l_x_minus_axis = "-0"
input_l_y_plus_axis = "+1"
input_l_y_minus_axis = "-1"
input_r_x_plus_axis = "+3"
input_r_x_minus_axis = "-3"
input_r_y_plus_axis = "+4"
input_r_y_minus_axis = "-4"
"#;

const PLAYSTATION_UDEV: &str = r#"input_driver = "udev"
input_device = "Sony Interactive Entertainment Wireless Controller"
input_device_display_name = "Steamless Controller (PlayStation)"
input_vendor_id = "1356"
input_product_id = "2508"
input_b_btn = "0"
input_a_btn = "1"
input_x_btn = "2"
input_y_btn = "3"
input_l_btn = "4"
input_r_btn = "5"
input_l2_btn = "6"
input_r2_btn = "7"
input_select_btn = "8"
input_start_btn = "9"
input_menu_toggle_btn = "10"
input_l3_btn = "11"
input_r3_btn = "12"
input_up_btn = "h0up"
input_down_btn = "h0down"
input_left_btn = "h0left"
input_right_btn = "h0right"
input_l2_axis = "+2"
input_r2_axis = "+5"
input_l_x_plus_axis = "+0"
input_l_x_minus_axis = "-0"
input_l_y_plus_axis = "+1"
input_l_y_minus_axis = "-1"
input_r_x_plus_axis = "+3"
input_r_x_minus_axis = "-3"
input_r_y_plus_axis = "+4"
input_r_y_minus_axis = "-4"
"#;

const FILES: [(&str, &str, &str); 3] = [
    ("udev", "Steamless Controller (Xbox 360).cfg", XBOX_UDEV),
    ("udev", "Steamless Controller (PlayStation).cfg", PLAYSTATION_UDEV),
    ("linuxraw", "Steamless Controller (Xbox 360).cfg", XBOX_LINUXRAW),
];

/// RetroArch's settings folders that exist on this computer (the normal
/// install, which Steam's RetroArch also uses, and the Flatpak one).
fn config_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    [
        dirs::config_dir().unwrap_or_else(|| home.join(".config")).join("retroarch"),
        home.join(".var/app/org.libretro.RetroArch/config/retroarch"),
    ]
    .into_iter()
    .filter(|d| d.join("retroarch.cfg").exists())
    .collect()
}

/// Where a RetroArch looks for controller profiles: `joypad_autoconfig_dir`
/// in its retroarch.cfg, or `autoconfig` beside it.
fn autoconfig_dir(config: &std::path::Path) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let fallback = config.join("autoconfig");
    let Ok(text) = std::fs::read_to_string(config.join("retroarch.cfg")) else {
        return fallback;
    };
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "joypad_autoconfig_dir" {
            continue;
        }
        let v = value.trim().trim_matches('"');
        if v.is_empty() || v == "default" {
            return fallback;
        }
        if let Some(rest) = v.strip_prefix(":/") {
            return config.join(rest);
        }
        if let Some(rest) = v.strip_prefix("~/") {
            return home.join(rest);
        }
        return PathBuf::from(v);
    }
    fallback
}

pub fn found() -> bool {
    !config_dirs().is_empty()
}

/// Whether every RetroArch found already has our profiles.
pub fn installed() -> bool {
    let dirs = config_dirs();
    !dirs.is_empty()
        && dirs.iter().all(|c| {
            let a = autoconfig_dir(c);
            FILES.iter().all(|(drv, name, _)| a.join(drv).join(name).exists())
        })
}

/// Writes the profiles into every RetroArch found. Returns the folders written.
pub fn install() -> std::io::Result<Vec<PathBuf>> {
    let dirs = config_dirs();
    if dirs.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "RetroArch's settings were not found"));
    }
    let mut written = Vec::new();
    for c in dirs {
        let a = autoconfig_dir(&c);
        for (drv, name, text) in FILES {
            let dir = a.join(drv);
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join(name), text)?;
        }
        crate::eventlog::write(format!("RETROARCH: controller profiles written to {}", a.display()));
        written.push(a);
    }
    Ok(written)
}
