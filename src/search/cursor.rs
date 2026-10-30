use super::{Filters, SearchError, SearchRequest};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

/// A client-held navigation position, NOT an authenticated database snapshot.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    pub filters: Filters,
    pub limit: usize,
    pub height: u64,
    pub wallet: usize,
    pub page: u32,
    pub transaction: usize,
    pub event: usize,
    pub total: Option<u64>,
}
impl Cursor {
    pub fn read(request: &SearchRequest, height: u64) -> Result<Self, SearchError> {
        let cursor = if let Some(text) = &request.cursor {
            let bytes = URL_SAFE_NO_PAD
                .decode(text)
                .map_err(|_| SearchError::InvalidCursor)?;
            serde_json::from_slice::<Self>(&bytes).map_err(|_| SearchError::InvalidCursor)?
        } else {
            Self {
                version: 1,
                node: None,
                filters: request.filters.clone(),
                limit: request.limit,
                height,
                wallet: 0,
                page: 1,
                transaction: 0,
                event: 0,
                total: None,
            }
        };
        if cursor.version != 1
            || cursor.filters != request.filters
            || cursor.limit != request.limit
            || cursor.height == 0
            || cursor.height > height
            || cursor.wallet >= cursor.wallets().len()
            || cursor.page == 0
            || cursor.transaction >= 20
        {
            return Err(SearchError::InvalidCursor);
        }
        Ok(cursor)
    }
    pub fn wallets(&self) -> &[String] {
        if self.filters.buyers.is_empty() {
            &self.filters.hosts
        } else {
            &self.filters.buyers
        }
    }
    pub fn next_wallet(&mut self) {
        self.wallet += 1;
        self.page = 1;
        self.transaction = 0;
        self.event = 0;
        self.total = None;
    }
    pub fn next_transaction(&mut self, page_len: usize) -> Result<(), SearchError> {
        self.event = 0;
        self.transaction += 1;
        if self.transaction >= page_len {
            if u64::from(self.page - 1) * 20
                + u64::try_from(page_len).map_err(|_| SearchError::Unavailable)?
                >= self.total.unwrap_or_default()
            {
                self.next_wallet();
            } else {
                self.page = self.page.checked_add(1).ok_or(SearchError::Unavailable)?;
                self.transaction = 0;
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Option<String>, SearchError> {
        if self.wallet >= self.wallets().len() {
            return Ok(None);
        }
        Ok(Some(URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(self).map_err(|_| SearchError::Unavailable)?,
        )))
    }
}
