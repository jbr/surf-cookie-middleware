#![forbid(unsafe_code, future_incompatible)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    nonstandard_style,
    missing_copy_implementations,
    unused_qualifications
)]

//! # A middleware for sending received cookies in surf
//!
//! see [`CookieMiddleware`] for details
//!
use async_dup::{Arc, Mutex};
use async_std::{
    fs::{File, OpenOptions},
    prelude::*,
    sync::RwLock,
};
use std::{
    convert::TryInto,
    io::{self, Cursor, SeekFrom},
    path::PathBuf,
};
use surf::{
    http::headers::{COOKIE, SET_COOKIE},
    middleware::{Middleware, Next},
    utils::async_trait,
    Client, Request, Response, Result, Url,
};

pub use cookie_store;
pub use cookie_store::CookieStore;

/// # A middleware for sending received cookies in surf
///
/// ## File system persistence
///
/// This middleware can optionally be constructed with a file or path
/// to enable writing "persistent cookies" to disk after every
/// received response.
///
/// ## Cloning semantics
///
/// All clones of this middleware will refer to the same data and fd
/// (if persistence is enabled).
///
/// # Usage example
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

#[derive(Default, Clone, Debug)]
pub struct CookieMiddleware {
    cookie_store: Arc<RwLock<CookieStore>>,
    file: Option<Arc<Mutex<File>>>,
}

#[async_trait]
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
    /// use surf_cookie_middleware::CookieMiddleware;
    ///
    /// let client = surf::Client::new().with(CookieMiddleware::new());
    ///
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
    /// use surf_cookie_middleware::{CookieStore, CookieMiddleware};
    ///
    /// let cookie_store = CookieStore::default();
    /// let client = surf::Client::new()
    ///     .with(CookieMiddleware::with_cookie_store(cookie_store));
    /// ```
    pub fn with_cookie_store(cookie_store: CookieStore) -> Self {
        Self {
            cookie_store: Arc::new(RwLock::new(cookie_store)),
            file: None,
        }
    }

    /// Builds a CookieMiddleware from a path to a filesystem cookie
    /// jar. These jars are stored in [ndjson](http://ndjson.org/)
    /// format. If the file does not exist, it will be created. If the
    /// file does exist, the cookie jar will be initialized with those
    /// cookies.
    ///
    /// Currently this only persists "persistent cookies" -- cookies
    /// with an expiry. "Session cookies" (without an expiry) are not
    /// persisted to disk, nor are expired cookies.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> std::io::Result<()> { async_std::task::block_on(async {
    /// use surf_cookie_middleware::{CookieStore, CookieMiddleware};
    ///
    /// let cookie_store = CookieStore::default();
    /// let client = surf::Client::new()
    ///     .with(CookieMiddleware::from_path("./cookies.ndjson").await?);
    /// # Ok(()) }) }
    /// ```
    pub async fn from_path(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .await?;

        Self::from_file(file).await
    }

    async fn load_from_file(file: &mut File) -> Option<CookieStore> {
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.ok();
        CookieStore::load_json(Cursor::new(&buf[..])).ok()
    }

    /// Builds a CookieMiddleware from a File (either
    /// [`async_std::fs::File`] or [`std::fs::File`]) that represents
    /// a filesystem cookie jar. These jars are stored in
    /// [ndjson](http://ndjson.org/) format. The cookie jar will be
    /// initialized with any cookies contained in this file, and
    /// persisted to the file after every request.
    ///
    /// Currently this only persists "persistent cookies" -- cookies
    /// with an expiry. "Session cookies" (without an expiry) are not
    /// persisted to disk, nor are expired cookies.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> std::io::Result<()> { async_std::task::block_on(async {
    /// use surf::Client;
    /// use surf_cookie_middleware::{CookieStore, CookieMiddleware};
    /// let cookie_store = CookieStore::default();
    /// let file = std::fs::File::create("./cookies.ndjson")?;
    /// let client = Client::new()
    ///     .with(CookieMiddleware::from_file(file).await?);
    /// # Ok(()) }) }
    /// ```
    pub async fn from_file(file: impl Into<File>) -> io::Result<Self> {
        let mut file = file.into();
        let cookie_store = Self::load_from_file(&mut file).await;
        Ok(Self {
            file: Some(Arc::new(Mutex::new(file))),
            cookie_store: Arc::new(RwLock::new(cookie_store.unwrap_or_default())),
        })
    }

    async fn save(&self) -> Result<()> {
        if let Some(ref file) = self.file {
            let mut string: Vec<u8> = vec![0];
            let mut cursor = Cursor::new(&mut string);

            self.cookie_store
                .read()
                .await
                .save_json(&mut cursor)
                .unwrap();

            let mut file = file.lock();
            file.seek(SeekFrom::Start(0)).await?;
            file.write_all(&string[..]).await?;
            file.set_len(string.len().try_into()?).await?;
            file.sync_all().await?;
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
                match cookie_store.parse(cookie.as_str(), request_url) {
                    Ok(action) => log::trace!("cookie action: {:?}", action),
                    Err(e) => log::trace!("cookie parse error: {:?}", e),
                }
            }
        }

        self.save().await?;

        Ok(())
    }
}
