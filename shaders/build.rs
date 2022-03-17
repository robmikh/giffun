use std::process::Command;

fn main() {
    let shader_folder = format!("{}/{}", std::env::var("OUT_DIR").unwrap(), "shaders");
    ensure_generated_dirs(&shader_folder).unwrap();

    compile_shader(&shader_folder, "cs_5_0", "LUTGeneration");
    compile_shader(&shader_folder, "ps_5_0", "LUTLookup_PS");
    compile_shader(&shader_folder, "vs_5_0", "LUTLookup_VS");
    compile_shader(&shader_folder, "cs_5_0", "TextureDiff");
}

fn compile_shader(shader_folder: &str, profile: &str, file_stem: &str) {
    println!("cargo:rerun-if-changed=src/{}.hlsl", file_stem);
    let pdb_out_dir = { format!("{}/{}.pdb", shader_folder, file_stem) };
    let mut lut_generation_command = Command::new("fxc");
    let status = lut_generation_command
        .args([
            "/Zi",
            "/Zss",
            "/T",
            profile,
            "/Fd",
            &pdb_out_dir,
            "/Fo",
            &format!("{}/{}.cso", shader_folder, file_stem),
            &format!("src/{}.hlsl", file_stem),
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

fn ensure_generated_dirs(shader_folder: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(shader_folder)
}
