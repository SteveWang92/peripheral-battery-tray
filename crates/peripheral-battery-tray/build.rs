//! Embeds the application icon and version information in the executable.

#[path = "src/app_icon.rs"]
mod app_icon;

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/app_icon.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let ico = out.join("app.ico");
    fs::write(&ico, app_icon::ico()).expect("OUT_DIR is writable");

    let version = env::var("CARGO_PKG_VERSION").expect("cargo sets the version");
    let numeric: Vec<&str> = version
        .split(['.', '-', '+'])
        .take(3)
        .chain(["0"])
        .collect();
    let numeric = numeric.join(",");
    let description = env::var("CARGO_PKG_DESCRIPTION").expect("the manifest has a description");
    // The resource compiler reads backslashes as escapes; forward slashes are paths too.
    let ico = ico.display().to_string().replace('\\', "/");
    let rc = format!(
        r#"1 ICON "{ico}"

1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "peripheral-battery-tray"
      VALUE "FileVersion", "{version}"
      VALUE "ProductName", "peripheral-battery-tray"
      VALUE "ProductVersion", "{version}"
      VALUE "Comments", "{description}"
      VALUE "OriginalFilename", "peripheral-battery-tray.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let rc_path = out.join("app.rc");
    fs::write(&rc_path, rc).expect("OUT_DIR is writable");
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("the resource compiler runs");
}
