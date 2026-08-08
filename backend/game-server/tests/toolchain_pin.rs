//! Rustの版が、指定している3箇所で食い違っていないことを確かめる。
//!
//! `rust-toolchain.toml` が正だが、DockerイメージのタグとCIのバージョン指定は
//! そこから自動では決まらない。ずれると次のような形で表に出る。
//!
//! - Dockerのタグがずれる: rustupがビルド中にツールチェーンを丸ごと取り直し、
//!   イメージのビルドが遅くなる。しかも取りに行けない環境では失敗する
//! - CIのバージョンがずれる: アクションが入れた版と rust-toolchain.toml が
//!   要求する版の両方をダウンロードすることになる
//!
//! どちらも「動くけれど無駄で、いずれ壊れる」類なので、気付ける形にしておく。

use std::{fs, path::PathBuf};

fn repository_root() -> PathBuf {
    // backend/game-server から2階層上がリポジトリ直下。
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = repository_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// `channel = "1.92.0"` のような行から版を取り出す。
fn pinned_channel(toolchain_file: &str) -> String {
    toolchain_file
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("channel"))
        .and_then(|rest| rest.split('"').nth(1))
        .expect("rust-toolchain.toml に channel の指定が無い")
        .to_string()
}

/// `FROM rust:1.92-bookworm AS builder` から `1.92` を取り出す。
fn dockerfile_versions(dockerfile: &str) -> Vec<String> {
    dockerfile
        .lines()
        .filter_map(|line| line.trim().strip_prefix("FROM rust:"))
        .filter_map(|rest| rest.split('-').next())
        .map(str::to_string)
        .collect()
}

/// `RUST_VERSION: "1.92.0"` から版を取り出す。
fn workflow_version(workflow: &str) -> String {
    workflow
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("RUST_VERSION:"))
        .and_then(|rest| rest.split('"').nth(1))
        .expect("ci.yml に RUST_VERSION の指定が無い")
        .to_string()
}

#[test]
fn toolchain_is_pinned_consistently() {
    let channel = pinned_channel(&read("rust-toolchain.toml"));

    // Dockerイメージのタグはパッチ版まで書かないので、前方一致で見る。
    // rust:1.92-bookworm が提供するのは 1.92.0 系である。
    for relative in ["deploy/docker/Dockerfile.dev", "deploy/docker/Dockerfile"] {
        let versions = dockerfile_versions(&read(relative));
        assert!(!versions.is_empty(), "{relative} に FROM rust: の行が無い");
        for version in versions {
            assert!(
                channel.starts_with(&version),
                "{relative} のイメージは rust:{version} だが、\
                 rust-toolchain.toml は {channel} を要求している。\
                 別名扱いになり、ビルドのたびにツールチェーンを取り直す"
            );
        }
    }

    let workflow = workflow_version(&read(".github/workflows/ci.yml"));
    assert_eq!(
        workflow, channel,
        "ci.yml の RUST_VERSION と rust-toolchain.toml の channel が違う。\
         CIで両方の版をダウンロードすることになる"
    );
}

/// 整形とlintに使うcomponentが、指定から抜け落ちていないこと。
///
/// 抜けると `make fmt-check` や `make lint` が
/// 「component が無い」で落ちる。新しい環境ほど踏みやすい。
#[test]
fn toolchain_pins_the_components_the_checks_need() {
    let toolchain_file = read("rust-toolchain.toml");
    for component in ["rustfmt", "clippy"] {
        assert!(
            toolchain_file.contains(component),
            "rust-toolchain.toml に {component} が入っていない"
        );
    }
}
