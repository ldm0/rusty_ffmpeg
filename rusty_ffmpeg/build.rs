use bindgen::{self, callbacks, Bindings, CargoCallbacks};
use once_cell::sync::Lazy;

use std::{collections::HashSet, env, fs};

/// Whitelist of the headers we want to generate bindings
static HEADERS: Lazy<[&str; 64]> = Lazy::new(|| {
    [
        "libavcodec/avcodec.h",
        "libavcodec/avfft.h",
        "libavcodec/dv_profile.h",
        "libavcodec/vaapi.h",
        "libavcodec/vorbis_parser.h",
        "libavdevice/avdevice.h",
        "libavfilter/avfilter.h",
        "libavfilter/buffersink.h",
        "libavfilter/buffersrc.h",
        "libavformat/avformat.h",
        "libavformat/avio.h",
        "libavutil/adler32.h",
        "libavutil/aes.h",
        "libavutil/audio_fifo.h",
        "libavutil/avstring.h",
        "libavutil/avutil.h",
        "libavutil/base64.h",
        "libavutil/blowfish.h",
        "libavutil/bprint.h",
        "libavutil/buffer.h",
        "libavutil/camellia.h",
        "libavutil/cast5.h",
        "libavutil/channel_layout.h",
        "libavutil/cpu.h",
        "libavutil/crc.h",
        "libavutil/dict.h",
        "libavutil/display.h",
        "libavutil/downmix_info.h",
        "libavutil/error.h",
        "libavutil/eval.h",
        "libavutil/fifo.h",
        "libavutil/file.h",
        "libavutil/frame.h",
        "libavutil/hash.h",
        "libavutil/hmac.h",
        "libavutil/imgutils.h",
        "libavutil/lfg.h",
        "libavutil/log.h",
        "libavutil/macros.h",
        "libavutil/mathematics.h",
        "libavutil/md5.h",
        "libavutil/mem.h",
        "libavutil/motion_vector.h",
        "libavutil/murmur3.h",
        "libavutil/opt.h",
        "libavutil/parseutils.h",
        "libavutil/pixdesc.h",
        "libavutil/pixfmt.h",
        "libavutil/random_seed.h",
        "libavutil/rational.h",
        "libavutil/replaygain.h",
        "libavutil/ripemd.h",
        "libavutil/samplefmt.h",
        "libavutil/sha.h",
        "libavutil/sha512.h",
        "libavutil/stereo3d.h",
        "libavutil/threadmessage.h",
        "libavutil/time.h",
        "libavutil/timecode.h",
        "libavutil/twofish.h",
        "libavutil/xtea.h",
        "libpostproc/postprocess.h",
        "libswresample/swresample.h",
        "libswscale/swscale.h",
    ]
});

/// Filter out all symbols in the HashSet, and for others things it will act
/// exactly the same as `CargoCallback`.
#[derive(Debug)]
struct FilterCargoCallbacks {
    inner: CargoCallbacks,
    emitted_macro: HashSet<String>,
}

impl FilterCargoCallbacks {
    fn new(set: HashSet<String>) -> Self {
        Self {
            inner: CargoCallbacks,
            emitted_macro: set,
        }
    }
}

impl callbacks::ParseCallbacks for FilterCargoCallbacks {
    fn will_parse_macro(&self, name: &str) -> callbacks::MacroParsingBehavior {
        if self.emitted_macro.contains(name) {
            callbacks::MacroParsingBehavior::Ignore
        } else {
            callbacks::MacroParsingBehavior::Default
        }
    }
}

fn generate_bindings(headers_dir: &str, headers: &[&str]) -> Result<Bindings, ()> {
    // Because the strange `FP_*` in `math.h` https://github.com/rust-lang/rust-bindgen/issues/687
    let filter_callback = FilterCargoCallbacks::new(
        vec![
            "FP_NAN".to_owned(),
            "FP_INFINITE".to_owned(),
            "FP_ZERO".to_owned(),
            "FP_SUBNORMAL".to_owned(),
            "FP_NORMAL".to_owned(),
        ]
        .into_iter()
        .collect(),
    );

    // Bindgen the headers
    headers
        .iter()
        // map header short path to full path
        .map(|header| format!("{}/{}", headers_dir, header))
        .fold(
            bindgen::builder()
                // Add clang path, for `#include` header finding in bindgen process.
                .clang_arg(format!("-I{}", headers_dir))
                .parse_callbacks(Box::new(filter_callback)),
            |builder, header| builder.header(header),
        )
        .generate()
}

fn main() {
    /* Workaround of cargo rerun-if-env-changed bug
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=FFMPEG_PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=FFMPEG_DYNAMIC_LINKING");
    */

    let out_dir = &env::var("OUT_DIR").unwrap();
    let ffmpeg_headers_dir = &env::var("FFMPEG_HEADERS_DIR").unwrap();
    let docs_rs = env::var("DOCS_RS").is_ok();
    let binding_file_path = &format!("{}/binding.rs", out_dir);

    // If it's a documentation generation from docs.rs, just copy the bindings
    // generated locally to `OUT_DIR`. We do this because the building
    // environment of docs.rs doesn't have an network connection, so we cannot
    // git clone the FFmpeg. And they also have a limitation on crate's size:
    // 10MB, which is not enough to fit in FFmpeg source files. So the only
    // thing we can do is copying the locally generated binding files to the
    // `OUT_DIR`.
    if docs_rs {
        fs::copy("src/binding.rs", binding_file_path)
            .expect("Prebuilt binding file failed to be copied.");
        return;
    }

    generate_bindings(&ffmpeg_headers_dir, &*HEADERS)
        .expect("Binding generation failed.")
        // Is it correct to generate binding to one file? :-/
        .write_to_file(binding_file_path)
        .expect("Cannot write binding to file.")
}
