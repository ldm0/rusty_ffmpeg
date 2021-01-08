use once_cell::sync::Lazy;

use std::{env, path};

/// All the libs that FFmpeg has
static LIBS: Lazy<[&str; 8]> = Lazy::new(|| {
    [
        "libavcodec",
        "libavdevice",
        "libavfilter",
        "libavformat",
        "libavutil",
        "libpostproc",
        "libswresample",
        "libswscale",
    ]
});

#[cfg(not(target_os = "windows"))]
mod non_windows {
    use super::*;
    use std::process::Command;

    fn try_probe_system_ffmpeg(library_names: &[&str]) -> Result<(), String> {
        match library_names.iter().find(|libname| {
            pkg_config::Config::new()
                // Remove side effect by disable metadata emitting
                .cargo_metadata(false)
                .probe(&libname)
                .is_err()
        }) {
            Some(&libname) => Err(libname.to_string()),
            None => Ok(()),
        }
    }

    fn clone_and_build_ffmpeg(out_dir: &str) {
        let ffmpeg_dir = &format!("{}/ffmpeg", out_dir);

        // Check if FFmpeg is cloned.
        if !path::PathBuf::from(format!("{}/fftools", ffmpeg_dir)).is_dir() {
            Command::new("git")
                .current_dir(out_dir)
                .args(["clone", "https://github.com/ffmpeg/ffmpeg", "--depth", "1"].iter())
                .spawn()
                .expect("Failed to clone FFmpeg submodule.")
                .wait()
                .expect("Failed to clone FFmpeg submodule.");
        }

        let path = &format!("{}/build/bin:{}", ffmpeg_dir, env::var("PATH").unwrap());

        // All outputs are stored in ./ffmpeg/build/{bin, lib, share, include}
        // If no prebuilt FFmpeg libraries provided, we build a custom FFmpeg.

        // Corresponding to the shell script below:
        // ./configure \
        //     --prefix="$PWD/build" \
        //     --extra-cflags="-I$PWD/build/include" \
        //     --extra-ldflags="-L$PWD/build/lib" \
        //     --bindir="$PWD/build/bin" \
        //     --pkg-config-flags="--static" \
        //     --extra-libs="-lpthread -lm" \
        //     --enable-gpl \
        //     --enable-libass \
        //     --enable-libfdk-aac \
        //     --enable-libfreetype \
        //     --enable-libmp3lame \
        //     --enable-libopus \
        //     --enable-libvorbis \
        //     --enable-libvpx \
        //     --enable-libx264 \
        //     --enable-libx265 \
        //     --enable-nonfree
        Command::new(format!("{}/configure", ffmpeg_dir))
            .current_dir(ffmpeg_dir)
            .env("PATH", path)
            .env(
                "PKG_CONFIG_PATH",
                format!("{}/build/lib/pkgconfig", ffmpeg_dir),
            )
            .args(
                [
                    format!(r#"--prefix={}/build"#, ffmpeg_dir),
                    format!(r#"--extra-cflags=-I{}/build/include"#, ffmpeg_dir),
                    format!(r#"--extra-ldflags=-L{}/build/lib"#, ffmpeg_dir),
                    format!(r#"--bindir={}/build/bin"#, ffmpeg_dir),
                ]
                .iter(),
            )
            .args(
                [
                    "--pkg-config-flags=--static",
                    "--extra-libs=-lpthread -lm",
                    "--enable-gpl",
                    "--enable-libass",
                    "--enable-libfdk-aac",
                    "--enable-libfreetype",
                    "--enable-libmp3lame",
                    "--enable-libopus",
                    "--enable-libvorbis",
                    "--enable-libvpx",
                    "--enable-libx264",
                    "--enable-libx265",
                    "--enable-nonfree",
                ]
                .iter(),
            )
            .spawn()
            .expect("FFmpeg build process: configure failed!")
            .wait()
            .expect("FFmpeg build process: configure failed!");

        let num_cpus = num_cpus::get();

        Command::new("make")
            .current_dir(ffmpeg_dir)
            .env("PATH", path)
            .arg(format!("-j{}", num_cpus))
            .spawn()
            .expect("FFmpeg build process: make compile failed!")
            .wait()
            .expect("FFmpeg build process: make compile failed!");

        Command::new("make")
            .current_dir(ffmpeg_dir)
            .arg(format!("-j{}", num_cpus))
            .arg("install")
            .spawn()
            .expect("FFmpeg build process: make install failed!")
            .wait()
            .expect("FFmpeg build process: make install failed!");
    }

    fn link_libraries(
        library_names: &[&str],
        ffmpeg_pkg_config_path: Option<String>,
        is_static: bool,
    ) {
        let previous_pkg_config_path = env::var("PKG_CONFIG_PATH").ok();
        if let Some(path) = ffmpeg_pkg_config_path {
            // for pkg-config
            env::set_var("PKG_CONFIG_PATH", path);
        } else {
            env::remove_var("PKG_CONFIG_PATH");
        }
        // TODO: if specific library is not enabled, we should not probe it. If we
        // want to implement this, we Should modify try_probe_system_ffmpeg() too.
        for libname in library_names {
            let _ = pkg_config::Config::new()
                // currently only support building with static libraries.
                .statik(is_static)
                .cargo_metadata(true)
                .probe(&libname)
                .unwrap_or_else(|_| panic!(format!("{} not found!", libname)));
        }

        if let Some(path) = previous_pkg_config_path {
            env::set_var("PKG_CONFIG_PATH", path);
        } else {
            env::remove_var("PKG_CONFIG_PATH");
        }
    }

    pub fn link_ffmpeg(library_names: &[&str], out_dir: &str, is_static: bool) {
        let ffmpeg_pkg_config_path = match env::var("FFMPEG_PKG_CONFIG_PATH") {
            Ok(x) => Some(x),
            Err(_) => {
                match try_probe_system_ffmpeg(library_names) {
                    Ok(_) => None,
                    Err(libname) => {
                        // If no system FFmpeg found, download and build one
                        eprintln!(
                            "{} not found in system path, let's git clone it and build.",
                            libname
                        );
                        clone_and_build_ffmpeg(out_dir);
                        Some(format!("{}/ffmpeg/build/lib/pkgconfig", out_dir))
                    }
                }
            }
        };
        // Now we can ensure available FFmpeg libraries.

        // Probe libraries(enable emitting cargo metadata)
        link_libraries(library_names, ffmpeg_pkg_config_path, is_static);
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    pub fn link_ffmpeg(
        _library_names: &[&str],
        _out_dir: &str,
        is_static: bool,
    ) -> HashSet<path::PathBuf> {
        let vcpkgrs_dynamic = env::var("VCPKGRS_DYNAMIC").ok();
        if is_static {
            env::remove_var("VCPKGRS_DYNAMIC");
        } else {
            env::set_var("VCPKGRS_DYNAMIC", "1");
        }

        vcpkg::Config::new()
            .find_package("ffmpeg")
            .unwrap();

        if let Some(x) = vcpkgrs_dynamic {
            env::set_var("VCPKGRS_DYNAMIC", x);
        } else {
            env::remove_var("VCPKGRS_DYNAMIC");
        }
        include_paths
    }
}

fn main() {
    /* Workaround of cargo rerun-if-env-changed bug
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=FFMPEG_PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=FFMPEG_DYNAMIC_LINKING");
    */

    let is_static = env::var("FFMPEG_DYNAMIC_LINKING").is_err();
    let out_dir = &env::var("OUT_DIR").unwrap();

    #[cfg(not(target_os = "windows"))]
    use non_windows::link_ffmpeg;
    #[cfg(target_os = "windows")]
    use windows::link_ffmpeg;
    link_ffmpeg(&*LIBS, out_dir, is_static);
}
