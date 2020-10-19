use async_lock::RwLock;
use cookie_store::CookieStore;
use std::sync::Arc;
use surf::http::headers::{COOKIE, SET_COOKIE};
use surf::middleware::{Middleware, Next};
use surf::{Client, Request, Response, Result, Url};

#[derive(Default, Clone)]
pub struct CookieMiddleware {
    cookie_store: Arc<RwLock<CookieStore>>,
}

#[surf::utils::async_trait]
impl Middleware for CookieMiddleware {
    async fn handle(&self, mut req: Request, client: Client, next: Next<'_>) -> Result<Response> {
        let url = req.url().clone();
        self.set_cookies(&mut req).await;
        let res = next.run(req, client).await?;
        self.store_cookies(&url, &res).await;
        Ok(res)
    }
}

impl CookieMiddleware {
    pub fn new() -> Self {
        Self::with_cookie_store(Default::default())
    }

    pub fn with_cookie_store(cookie_store: CookieStore) -> Self {
        Self {
            cookie_store: Arc::new(RwLock::new(cookie_store)),
        }
    }

    pub async fn set_cookies(&self, req: &mut Request) {
        let cookie_store = self.cookie_store.read().await;
        let mut matches = cookie_store.matches(req.url());

        // clients "SHOULD" sort by path length
        matches.sort_by(|a, b| b.path.len().cmp(&a.path.len()));

        let values = matches
            .iter()
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ");

        req.insert_header(COOKIE, values);
    }

    pub async fn store_cookies(&self, request_url: &Url, res: &Response) {
        if let Some(set_cookies) = res.header(SET_COOKIE) {
            let mut cookie_store = self.cookie_store.write().await;
            for cookie in set_cookies {
                match cookie_store.parse(cookie.as_str(), &request_url) {
                    Ok(action) => log::trace!("cookie action: {:?}", action),
                    Err(e) => log::trace!("cookie parse error: {:?}", e),
                }
            }
        }
    }
}
