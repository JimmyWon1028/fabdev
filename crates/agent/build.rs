use std::env;
use std::path::PathBuf;

fn main() {
  println!("cargo:rerun-if-changed=Info.plist");

  if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
    return;
  }

  let manifest_dir =
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not available"));
  let info_plist = manifest_dir.join("Info.plist");

  println!(
    "cargo:rustc-link-arg-bin=fabdev-agent=-Wl,-sectcreate,__TEXT,__info_plist,{}",
    info_plist.display()
  );
}
