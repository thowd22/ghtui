//! Retained daily Dashboard observations (TASK-61). Account-scoped, bounded and
//! created lazily on the first retained Dashboard load, so other flows never see
//! a schema change. Rows are evidence for trends, never access authority.
use super::*;

const DDL: &str = "CREATE TABLE IF NOT EXISTS metric_trend_points_v1(
    host TEXT NOT NULL, login TEXT NOT NULL, scope_key TEXT NOT NULL, card TEXT NOT NULL,
    day INTEGER NOT NULL CHECK(typeof(day)='integer' AND day>=0),
    payload TEXT NOT NULL,
    PRIMARY KEY(host,login,scope_key,card,day))";
/// Daily points older than this many days before the newest retained day are dropped.
pub const METRIC_TREND_RETAINED_DAYS: u64 = 90;
pub const METRIC_TREND_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_KEY_BYTES: usize = 64 * 300;

fn valid_key(text: &str) -> bool {
    !text.is_empty() && text.len() <= MAX_KEY_BYTES && !text.chars().any(char::is_control)
}

impl Cache {
    /// Upserts one UTC day's points (`card`, payload) for this account and scope key,
    /// then drops this account's points older than the retention bound.
    pub fn retain_metric_trend_points_v1(
        &self,
        scope: &Scope,
        scope_key: &str,
        day: u64,
        points: &[(String, Value)],
    ) -> Result<()> {
        if !valid_key(scope_key) || day > i64::MAX as u64 || points.len() > 64 {
            bail!("Metric trend point key or bounds are invalid; nothing retained");
        }
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute_batch(DDL)?;
        for (card, payload) in points {
            let text = serde_json::to_string(payload)?;
            if !valid_key(card) || text.len() > METRIC_TREND_PAYLOAD_BYTES {
                bail!("Metric trend point exceeds its byte budget; nothing retained");
            }
            transaction.execute(
                "INSERT INTO metric_trend_points_v1(host,login,scope_key,card,day,payload) VALUES(?1,?2,?3,?4,?5,?6)
                 ON CONFLICT(host,login,scope_key,card,day) DO UPDATE SET payload=excluded.payload",
                params![scope.host, scope.login, scope_key, card, day as i64, text],
            )?;
        }
        transaction.execute(
            "DELETE FROM metric_trend_points_v1 WHERE host=?1 AND login=?2 AND day<?3",
            params![
                scope.host,
                scope.login,
                day.saturating_sub(METRIC_TREND_RETAINED_DAYS) as i64
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// This account's retained points for one scope key in `[first_day, last_day]`,
    /// ordered by card and day. An absent table means nothing was retained yet.
    pub fn metric_trend_points_v1(
        &self,
        scope: &Scope,
        scope_key: &str,
        first_day: u64,
        last_day: u64,
    ) -> Result<Vec<(String, u64, Value)>> {
        if !valid_key(scope_key) || last_day > i64::MAX as u64 || first_day > last_day {
            bail!("Metric trend read bounds are invalid");
        }
        let exists: bool = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='metric_trend_points_v1')",
            [],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(Vec::new());
        }
        let mut statement = self.connection.prepare(
            "SELECT card,day,CASE WHEN length(CAST(payload AS BLOB))<=?6 THEN payload ELSE NULL END
             FROM metric_trend_points_v1 WHERE host=?1 AND login=?2 AND scope_key=?3 AND day BETWEEN ?4 AND ?5
             ORDER BY card,day LIMIT 4096",
        )?;
        let rows = statement.query_map(
            params![
                scope.host,
                scope.login,
                scope_key,
                first_day as i64,
                last_day as i64,
                METRIC_TREND_PAYLOAD_BYTES as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?;
        let mut output = Vec::new();
        for row in rows {
            let (card, day, payload) = row?;
            let payload = match payload {
                Some(text) => serde_json::from_str(&text).unwrap_or(Value::Null),
                None => Value::Null,
            };
            output.push((card, u64::try_from(day).unwrap_or(0), payload));
        }
        Ok(output)
    }
}
