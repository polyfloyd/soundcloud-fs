use serde::{self, Deserialize, Deserializer};

pub mod date {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Utc.datetime_from_str(&s, "%Y/%m/%d %H:%M:%S %z")
            .map_err(serde::de::Error::custom)
    }
}

pub mod empty_str_as_none {
    use super::*;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let o: Option<String> = Option::deserialize(deserializer)?;
        Ok(o.filter(|s| !s.is_empty()))
    }
}

pub mod null_as_false {
    use super::*;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let o: Option<bool> = Option::deserialize(deserializer)?;
        Ok(o.unwrap_or(false))
    }
}
