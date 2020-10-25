use serde_json::value::Value;
use surf::Client;
use surf_cookie_middleware::CookieMiddleware;

#[async_std::main]
async fn main() -> surf::Result<()> {
    Client::new()
        .with(CookieMiddleware::from_path("./example.ndjson").await?)
        .get("https://httpbin.org/response-headers?Set-Cookie=USER_ID=10;+Max-Age=1000")
        .await?;

    // no data shared in memory between the requests

    let cookies: Value = Client::new()
        .with(CookieMiddleware::from_path("./example.ndjson").await?)
        .get("https://httpbin.org/cookies")
        .recv_json()
        .await?;

    assert_eq!(
        cookies.get("cookies").unwrap().get("USER_ID").unwrap(),
        "10"
    );

    Ok(())
}
