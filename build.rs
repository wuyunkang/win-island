//! 把灵动岛图标作为 RT_GROUP_ICON #1 编进 exe，供 LoadImageW(hInst, 1, ...) 取用。
//! 不引入额外依赖：直接调用 Windows SDK 的 rc.exe，把 .res 交给链接器。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    let roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ];
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let rc = dir.join("x64").join("rc.exe");
            if rc.exists() {
                found.push(rc);
            }
        }
    }
    found.sort();
    found.pop()
}

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/island.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref()==Ok("gnu") {
        let directory=PathBuf::from(env::var("OUT_DIR").unwrap());
        // MinGW 没有自带 Windows 系统 winsqlite3 的导入库；只声明上游用到的四个系统符号。
        let definitions=directory.join("winsqlite3.def");
        fs::write(&definitions,"LIBRARY winsqlite3.dll\nEXPORTS\nsqlite3_open_v2\nsqlite3_exec\nsqlite3_busy_timeout\nsqlite3_close\n").unwrap();
        let imported=directory.join("libwinsqlite3.a");
        assert!(Command::new("llvm-dlltool").args(["-m","i386:x86-64","-d"]).arg(&definitions).arg("-l").arg(&imported).status().is_ok_and(|status|status.success()),"无法生成 Windows SQLite 导入库");
        println!("cargo:rustc-link-search=native={}",directory.display());
        let out=directory.join("win-island-resource.o");
        let status=Command::new("llvm-windres").args(["--codepage=65001","-i","app.rc","-O","coff","-o"]).arg(&out).status();
        if status.is_ok_and(|status|status.success()){println!("cargo:rustc-link-arg={}",out.display());}
        else {panic!("LLVM/MinGW 构建需要 llvm-windres，请使用 scripts/dev.ps1 配置工具路径");}
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("island.res");
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe 未找到，跳过图标资源");
        return;
    };
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&out)
        .arg("app.rc")
        .status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        println!("cargo:warning=rc.exe 编译 app.rc 失败");
        return;
    }
    println!("cargo:rustc-link-arg={}", out.display());
}
