use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct ContractEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub attributes: Vec<Attribute>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Attribute {
    pub key: String,
    pub value: String,
}

impl ContractEvent {
    /// Returns an attribute only when its key occurs exactly once.
    #[must_use]
    pub fn unique(&self, key: &str) -> Option<&str> {
        let mut values = self.attributes.iter().filter(|a| a.key == key);
        let value = &values.next()?.value;
        if values.next().is_some() {
            None
        } else {
            Some(value)
        }
    }

    /// Requires exactly one emitter; a matching `deal` attribute is not sufficient.
    #[must_use]
    pub fn emitted_by(&self, address: &str) -> bool {
        let mut emitters = self
            .attributes
            .iter()
            .filter(|a| a.key == "_contract_address");
        emitters.next().is_some_and(|a| a.value == address) && emitters.next().is_none()
    }
}
