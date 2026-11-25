#[tokio::main]
async fn main() -> auth_server::error::Result<()> {
    auth_server::app::run().await
}
