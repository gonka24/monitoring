use serde::Deserialize;

pub const ADDRESS: &str = "gonka1ea2s57dvs06fr5mjzmna9w9vt0xylrna5cjtmzr5h72vkrza3xxsp7eayy";

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct DealRef {
    pub id: u64,
    pub address: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct DealPage {
    pub deals: Vec<DealRef>,
}
