use async_std::fs;
use http::cookies::Cookie;
use http::headers::{COOKIE, SET_COOKIE};
use std::{convert::TryInto, path::Path, time::Duration};
use surf::{http, Client};
use surf_cookie_middleware::CookieMiddleware;
use tempfile::NamedTempFile;
use tide::Request;

fn build_app() -> tide::Server<()> {
    let mut server = tide::new();
    server
        .at("/persistent/:name/:value")
        .get(|req: Request<_>| async move {
            let name = req.param("name")?;
            let value = req.param("value")?;
            let mut res = tide::Response::new(200);
            res.insert_cookie(
                Cookie::build(name.to_string(), value.to_string())
                    .max_age(Duration::from_secs(100).try_into()?)
                    .path("/")
                    .finish(),
            );
            Ok(res)
        });

    server
        .at("/session/:name/:value)")
        .get(|req: Request<_>| async move {
            let name = req.param("name")?;
            let value = req.param("value")?;
            let mut res = tide::Response::new(200);
            res.insert_cookie(
                Cookie::build(name.to_string(), value.to_string())
                    .path("/")
                    .finish(),
            );
            Ok(res)
        });

    server
        .at("/cookies")
        .get(|req: Request<_>| async move { Ok(req[COOKIE].to_string()) });

    server
}

#[async_std::test]
async fn from_file_and_from_path() -> surf::Result<()> {
    let server = build_app();
    let (file, path) = NamedTempFile::new()?.into_parts();
    let path: &Path = path.as_ref();

    let middleware = CookieMiddleware::from_file(file).await?;
    let client = Client::with_http_client(server.clone()).with(middleware.clone());
    let res = client.get("http://_/persistent/name/value").await?;
    assert_eq!(res[SET_COOKIE], "name=value; Path=/; Max-Age=100");
    assert_eq!(fs::read_to_string(path).await?.lines().count(), 1);

    let res = client.get("http://_/persistent/other/other-value").await?;
    assert_eq!(res[SET_COOKIE], "other=other-value; Path=/; Max-Age=100");
    assert_eq!(fs::read_to_string(path).await?.lines().count(), 2);

    let middleware = CookieMiddleware::from_path(&path).await?;
    let client = Client::with_http_client(server).with(middleware.clone());
    let cookies = client.get("http://_/cookies").recv_string().await?;
    assert_eq!(cookies, r#"["name=value; other=other-value"]"#);

    Ok(())
}
