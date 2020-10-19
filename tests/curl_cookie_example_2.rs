use http::cookies::Cookie;
use http::headers::{COOKIE, SET_COOKIE};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use surf::http;

use tide::StatusCode;
use tide_testing::TideTestingExt;

use surf_cookie_middleware::CookieMiddleware;

// from https://curl.haxx.se/rfc/cookie_spec.html
fn build_app() -> tide::Server<Arc<AtomicUsize>> {
    let state = Arc::new(AtomicUsize::new(0));
    let mut server = tide::with_state(state);

    server
        .at("/")
        .get(|req: tide::Request<Arc<AtomicUsize>>| async move {
            let mut response = tide::Response::new(200);

            match &req.state().load(Ordering::Relaxed) {
                0 => {
                    response.insert_cookie(
                        Cookie::build("PART_NUMBER", "ROCKET_LAUNCHER_0001")
                            .path("/")
                            .finish(),
                    );
                }

                1 => {
                    assert_eq!(req[COOKIE], "PART_NUMBER=ROCKET_LAUNCHER_0001");
                    response.insert_cookie(
                        Cookie::build("PART_NUMBER", "RIDING_ROCKET_0023")
                            .path("/ammo")
                            .finish(),
                    );
                }

                _ => unreachable!(),
            }

            req.state().fetch_add(1, Ordering::Relaxed);

            Ok(response)
        });

    server
        .at("/ammo")
        .get(|req: tide::Request<Arc<AtomicUsize>>| async move {
            assert_eq!(
                req[COOKIE],
                "PART_NUMBER=RIDING_ROCKET_0023; PART_NUMBER=ROCKET_LAUNCHER_0001"
            );
            Ok(StatusCode::Ok)
        });

    server
}

#[async_std::test]
async fn it_works() -> surf::Result<()> {
    let app = build_app();
    let middleware = CookieMiddleware::new();
    let client = app.client().with(middleware.clone());

    let res = client.get("/").await?;
    assert_eq!(res[SET_COOKIE], "PART_NUMBER=ROCKET_LAUNCHER_0001; Path=/");

    let res = client.get("/").await?;
    assert_eq!(
        res[SET_COOKIE],
        "PART_NUMBER=RIDING_ROCKET_0023; Path=/ammo"
    );

    let res = client.get("/ammo").await?;
    assert!(res.header(SET_COOKIE).is_none());

    Ok(())
}
