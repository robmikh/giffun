use std::process::Command;

fn main() {
    compile_shader("cs_5_0", "LUTGeneration");
    compile_shader("ps_5_0", "LUTLookup_PS");
    compile_shader("vs_5_0", "LUTLookup_VS");
    compile_shader("cs_5_0", "TextureDiff");
}

fn compile_shader(profile: &str, file_stem: &str) {
    //println!("cargo:rerun-if-changed=data/shaders/{}.hlsl", file_stem);
    let pdb_out_dir = {
        //let mut pdb_out_dir = std::env::var("OUT_DIR").unwrap();
        //let last_char = pdb_out_dir.chars().last().unwrap();
        //if last_char != '/' && last_char != '\\' {
        //    pdb_out_dir.push('\\');
        //}
        //pdb_out_dir
        format!("data/generated/shaders/{}.pdb", file_stem)
    };
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
            &format!("data/generated/shaders/{}.cso", file_stem),
            &format!("data/shaders/{}.hlsl", file_stem),
        ])
        .status()
        .unwrap();
    assert!(status.success());
}
