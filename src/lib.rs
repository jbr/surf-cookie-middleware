#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_debug_implementations,
    nonstandard_style,
    missing_copy_implementations,
    unused_qualifications
)]

use async_dup::{Arc, Mutex};
use async_lock::RwLock;
use async_std::prelude::*;
use std::convert::TryInto;
use std::io::Cursor;
use surf::http::headers::{COOKIE, SET_COOKIE};
use surf::middleware::{Middleware, Next};
use surf::{Client, Request, Response, Result, Url};

pub use cookie_store;
pub use cookie_store::CookieStore;

#[derive(Default, Clone, Debug)]
pub struct CookieMiddleware {
    cookie_store: Arc<RwLock<CookieStore>>,
    file: Option<Arc<Mutex<async_std::fs::File>>>,
}

#[surf::utils::async_trait]
impl Middleware for CookieMiddleware {
    async fn handle(&self, mut req: Request, client: Client, next: Next<'_>) -> Result<Response> {
        let url = req.url().clone();
        self.set_cookies(&mut req).await;
        let res = next.run(req, client).await?;
        self.store_cookies(&url, &res).await?;
        Ok(res)
    }
}

impl CookieMiddleware {
    /// Builds a new CookieMiddleware
    ///
    /// # Example
    ///
    /// ```rust
    /// use surf::Client;
    /// use surf_cookie_middleware::CookieMiddleware;
    /// let client = Client::new().with(CookieMiddleware::new());
    /// // client.get(...).await?;
    /// // client.get(...).await?; <- this request will send any appropriate
    /// //                            cookies received from the first request,
    /// //                            based on request url
    /// ```

    pub fn new() -> Self {
        Self::with_cookie_store(Default::default())
    }

    /// Builds a CookieMiddleware with an existing [`cookie_store::CookieStore`]
    ///
    /// # Example
    ///
    /// ```rust
    /// use surf::Client;
    /// use surf_cookie_middleware::{CookieStore, CookieMiddleware};
    /// let cookie_store = CookieStore::default();
    /// let client = Client::new()
    ///     .with(CookieMiddleware::with_cookie_store(cookie_store));
    ///
    /// // client.get(...).await?;
    /// // client.get(...).await?; <- this request will send any appropriate
    /// //                            cookies received from the first request,
    /// //                            based on request url
    /// ```
    pub fn with_cookie_store(cookie_store: CookieStore) -> Self {
        Self {
            cookie_store: Arc::new(RwLock::new(cookie_store)),
            file: None,
        }
    }

    pub async fn from_path(path: impl Into<std::path::PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let mut file = async_std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .await?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await?;
        let cookie_store = CookieStore::load_json(Cursor::new(&buf[..])).unwrap_or_default();

        Ok(Self {
            file: Some(Arc::new(Mutex::new(file))),
            cookie_store: Arc::new(RwLock::new(cookie_store)),
        })
    }

    async fn save(&self) -> Result<()> {
        if let Some(ref file) = self.file {
            let mut string: Vec<u8> = vec![0];
            let mut cursor = std::io::Cursor::new(&mut string);

            self.cookie_store
                .read()
                .await
                .save_json(&mut cursor)
                .unwrap();

            let mut file = file.lock();
            file.seek(std::io::SeekFrom::Start(0)).await?;
            file.write_all(&string[..]).await?;
            file.set_len(string.len().try_into()?).await?;
        }
        Ok(())
    }

    async fn set_cookies(&self, req: &mut Request) {
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

    async fn store_cookies(&self, request_url: &Url, res: &Response) -> Result<()> {
        if let Some(set_cookies) = res.header(SET_COOKIE) {
            let mut cookie_store = self.cookie_store.write().await;
            for cookie in set_cookies {
                match cookie_store.parse(cookie.as_str(), &request_url) {
                    Ok(action) => log::trace!("cookie action: {:?}", action),
                    Err(e) => log::trace!("cookie parse error: {:?}", e),
                }
            }
        }

        self.save().await?;

        Ok(())
    }
}
