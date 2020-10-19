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
                        Cookie::build("CUSTOMER", "WILE_E_COYOTE")
                            .path("/")
                            .finish(),
                    );
                }

                1 => {
                    assert_eq!(req[COOKIE], "CUSTOMER=WILE_E_COYOTE");
                    response.insert_cookie(
                        Cookie::build("PART_NUMBER", "ROCKET_LAUNCHER_0001")
                            .path("/")
                            .finish(),
                    );
                }

                2 => {
                    assert_eq!(
                        req[COOKIE],
                        "CUSTOMER=WILE_E_COYOTE; PART_NUMBER=ROCKET_LAUNCHER_0001"
                    );
                    response
                        .insert_cookie(Cookie::build("SHIPPING", "FEDEX").path("/foo").finish());
                }

                3 => {
                    assert_eq!(
                        req[COOKIE],
                        "CUSTOMER=WILE_E_COYOTE; PART_NUMBER=ROCKET_LAUNCHER_0001"
                    );
                }

                _ => unreachable!(),
            }

            req.state().fetch_add(1, Ordering::Relaxed);

            Ok(response)
        });

    server
        .at("/foo")
        .get(|req: tide::Request<Arc<AtomicUsize>>| async move {
            assert_eq!(
                req[COOKIE],
                "SHIPPING=FEDEX; CUSTOMER=WILE_E_COYOTE; PART_NUMBER=ROCKET_LAUNCHER_0001"
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
    assert_eq!(res[SET_COOKIE], "CUSTOMER=WILE_E_COYOTE; Path=/");

    let res = client.get("/").await?;
    assert_eq!(res[SET_COOKIE], "PART_NUMBER=ROCKET_LAUNCHER_0001; Path=/");

    let res = client.get("/").await?;
    assert_eq!(res[SET_COOKIE], "SHIPPING=FEDEX; Path=/foo");

    let res = client.get("/").await?;
    assert!(res.header(SET_COOKIE).is_none());

    let res = client.get("/foo").await?;
    assert!(res.header(SET_COOKIE).is_none());

    Ok(())
}
