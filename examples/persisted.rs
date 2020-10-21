use serde_json::value::Value;
use surf::Client;
use surf_cookie_middleware::CookieMiddleware;

#[async_std::main]
async fn main() -> surf::Result<()> {
    let client = Client::new().with(CookieMiddleware::new());

    client
        .get("https://httpbin.org/cookies/set/USER_ID/10")
        .await?;

    let cookies: Value = client
        .get("https://httpbin.org/cookies")
        .recv_json()
        .await?;

    assert_eq!(
        cookies.get("cookies").unwrap().get("USER_ID").unwrap(),
        "10"
    );

    Ok(())
}
