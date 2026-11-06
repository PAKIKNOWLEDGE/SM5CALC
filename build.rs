use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mina = manifest.join("../etterna/src/Etterna/MinaCalc");
    let mina_cpp = mina.join("MinaCalc.cpp");
    if !mina_cpp.is_file() {
        panic!(
            "找不到 {}。把 SM5CALC 和 etterna 放在同一个上级目录里再编译。",
            mina_cpp.display()
        );
    }

    println!("cargo:rerun-if-changed={}", mina_cpp.display());
    println!("cargo:rerun-if-changed=native/minacalc_api.cpp");
    println!("cargo:rerun-if-changed=build.rs");

    let original = std::fs::read_to_string(&mina_cpp).expect("read MinaCalc.cpp");
    let needle = "ulbu_in_charge->load_calc_params_from_disk(true);";
    if !original.contains(needle) {
        panic!("MinaCalc.cpp 里的参数加载调用变了，build.rs 需要跟着改");
    }
    // Standalone builds do not have the XML parameter loader. Chart MSD uses
    // the compiled defaults, which is also what the game uses when no
    // CalcParams xml is loaded.
    let patched = original.replacen(needle, "(void)0;", 1);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let patched_path = out_dir.join("MinaCalc.standalone.cpp");
    std::fs::write(&patched_path, patched).expect("write patched MinaCalc");

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .file(&patched_path)
        .file(manifest.join("native/minacalc_api.cpp"))
        .include(&mina)
        .define("STANDALONE_CALC", None)
        .define("NDEBUG", None)
        .opt_level(2)
        .debug(false)
        .static_crt(true)
        .flag_if_supported("/bigobj")
        .flag_if_supported("/EHsc")
        .compile("minacalc");
}
