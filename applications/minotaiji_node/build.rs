// Copyright 2023, OnSight Tech Services LLC
// SPDX-License-Identifier: BSD-3-Clause

use taiji_features::resolver::build_features;

#[cfg(windows)]
fn main() {
    build_features();
    use std::env;
    println!("cargo:rerun-if-changed=icon.res");
    let mut path = env::current_dir().unwrap();
    path.push("icon.res");
    println!("cargo:rustc-link-arg={}", path.into_os_string().into_string().unwrap());
}

#[cfg(not(windows))]
pub fn main() {
    build_features();
    // Build as usual
}
