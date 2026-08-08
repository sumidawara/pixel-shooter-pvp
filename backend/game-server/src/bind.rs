//! 待受アドレスの決定。指定されたポートが埋まっていれば、その先を順に試す。
//!
//! 探索をサーバー側に置いているのは、`server.json` で挙動を決められるようにするため。
//! クライアント側にも同じ処理があると、設定を見ない方の実装が勝ってしまう。

use tokio::net::TcpListener;

/// `address`から順にポートを試し、待受けできたリスナーと実際のアドレスを返す。
///
/// `search_range`が0なら`address`だけを試す。ポートを固定したい場合に使う。
pub(crate) async fn listen_with_search(
    address: &str,
    search_range: u32,
) -> Result<(TcpListener, String), String> {
    let Some((host, port)) = split_host_port(address) else {
        return Err(format!("invalid bind address: {address}"));
    };

    let mut last_error = String::new();
    for offset in 0..=search_range {
        let Some(candidate_port) = port.checked_add(offset as u16) else {
            break;
        };
        let candidate = format!("{host}:{candidate_port}");
        match TcpListener::bind(&candidate).await {
            Ok(listener) => return Ok((listener, candidate)),
            Err(error) => {
                // 使用中以外の理由（権限不足など）は、先を試しても同じなので即やめる。
                if error.kind() != std::io::ErrorKind::AddrInUse {
                    return Err(format!("could not bind to {candidate}: {error}"));
                }
                last_error = format!("{error}");
            }
        }
    }

    if search_range == 0 {
        Err(format!("could not bind to {address}: {last_error}"))
    } else {
        Err(format!(
            "could not bind to {address} or the {search_range} ports after it: {last_error}"
        ))
    }
}

/// `127.0.0.1:9001` をホストとポートへ分ける。
fn split_host_port(address: &str) -> Option<(&str, u16)> {
    let (host, port) = address.rsplit_once(':')?;
    Some((host, port.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_host_and_port() {
        assert_eq!(split_host_port("127.0.0.1:9001"), Some(("127.0.0.1", 9001)));
        assert_eq!(split_host_port("0.0.0.0:80"), Some(("0.0.0.0", 80)));
        assert_eq!(split_host_port("127.0.0.1"), None);
        assert_eq!(split_host_port("127.0.0.1:notaport"), None);
    }

    #[tokio::test]
    async fn uses_the_requested_port_when_it_is_free() {
        let (_listener, address) = listen_with_search("127.0.0.1:0", 0)
            .await
            .expect("port 0 lets the OS choose");
        assert!(address.starts_with("127.0.0.1:"));
    }

    #[tokio::test]
    async fn moves_to_the_next_port_when_the_first_is_taken() {
        // OSに空きを選ばせてから、その番号を塞いだ状態を作る。
        let occupied = TcpListener::bind("127.0.0.1:0").await.expect("probe");
        let occupied_address = occupied.local_addr().expect("addr").to_string();
        let occupied_port: u16 = occupied_address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .expect("port");

        let (_listener, address) = listen_with_search(&occupied_address, 20)
            .await
            .expect("a later port should be free");
        let chosen: u16 = address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .expect("port");
        assert!(
            chosen > occupied_port,
            "使用中の番号を避けていない: {chosen} <= {occupied_port}"
        );
    }

    #[tokio::test]
    async fn does_not_move_when_search_is_disabled() {
        let occupied = TcpListener::bind("127.0.0.1:0").await.expect("probe");
        let occupied_address = occupied.local_addr().expect("addr").to_string();

        let error = listen_with_search(&occupied_address, 0)
            .await
            .expect_err("固定指定なので別の番号へ移ってはいけない");
        assert!(error.contains(&occupied_address));
    }
}
