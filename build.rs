fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=src/vst_host/seh_wrapper.c");
        cc::Build::new()
            .file("src/vst_host/seh_wrapper.c")
            .cpp(false)
            .compile("seh_wrapper");
    }
}
