//! 固定GameServerプールの管理とデバッグ画面を提供するAdminServer。

mod app;
mod routes;
mod state;

#[tokio::main]
async fn main() {
    app::run().await;
}
