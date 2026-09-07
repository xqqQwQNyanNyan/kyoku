fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 编译时内置样本，发行版首次启动即可写入牌谱库，不依赖源码路径。
    let samples = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
        .join("../../fixtures/tenhou");
    println!("cargo:rerun-if-changed={}", samples.display());
    let mut files = std::fs::read_dir(&samples)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.extension().is_some_and(|ext| ext == "json"));
    files.sort();
    let mut source = String::from("const EXAMPLES: &[(&str, &str)] = &[\n");
    for file in files {
        let name = file
            .file_name()
            .ok_or("示例文件名无效")?
            .to_str()
            .ok_or("示例文件名必须为 UTF-8")?;
        source.push_str(&format!(
            "({name:?}, include_str!({:?})),\n",
            file.to_str().ok_or("示例路径必须为 UTF-8")?
        ));
    }
    source.push_str("];\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("replay_examples.rs"),
        source,
    )?;
    tauri_build::build();
    Ok(())
}
