//! 標準出力と標準エラーをファイルへ向ける。
//!
//! 配布版では、GodotのCREATE ROOMが起動した子プロセスの出力がどこにも出ない。
//! 「部屋が作れない」と言われたときに手掛かりが何も残らないため、
//! ファイルへ残せるようにする。
//!
//! `println!` を置き換えるのではなくプロセスの出力そのものを差し替えているのは、
//! パニックのバックトレースまで含めて残したいため。原因が一番知りたいのは
//! 想定外の落ち方をしたときで、それは `println!` には現れない。

use std::{fs::File, path::Path};

/// 出力先を`path`へ切り替える。既存の内容へ追記する。
///
/// 切り替えられなかった場合はエラーを返す。呼び出し側は続行してよい。
/// ログが残らないだけで、サーバー自体は動く。
pub(crate) fn redirect_to_file(path: &Path) -> Result<(), String> {
    if let Some(directory) = path.parent()
        && !directory.as_os_str().is_empty()
    {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    }
    let file = File::options()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("could not open {}: {error}", path.display()))?;
    redirect(file)
}

#[cfg(unix)]
fn redirect(file: File) -> Result<(), String> {
    use std::os::fd::AsRawFd;

    let descriptor = file.as_raw_fd();
    // stdout(1) と stderr(2) の指す先をファイルへ差し替える。
    // これ以降は println! もパニックの出力も、この先へ流れる。
    for target in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        // SAFETY: descriptor は直前に開いた有効なファイル、
        // target は標準ストリームの番号。dup2 はどちらも閉じない。
        if unsafe { libc::dup2(descriptor, target) } == -1 {
            return Err(format!(
                "could not redirect fd {target}: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn redirect(_file: File) -> Result<(), String> {
    // Windowsでは標準ハンドルの差し替え方が異なる。必要になったときに実装する。
    // ここで黙って成功を返すと、ログが残っている前提で調べることになるので、
    // できないことをはっきり返す。
    Err("log file redirection is not implemented on this platform".into())
}
