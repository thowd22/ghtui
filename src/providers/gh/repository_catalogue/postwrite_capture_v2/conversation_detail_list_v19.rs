//! Five closed READ children on the SAME prepaid cursor. No endpoint/argument
//! getter, fresh pool, source grant, HTTP grammar or implicit retry is exposed.
use super::*;
use crate::providers::gh::ConversationDetailListV19;

impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh) fn conversation_detail_list_v19(
        &mut self,
        provider: &GithubProvider,
        parent: &PrResourceParentV1,
        list: ConversationDetailListV19,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        let endpoint = list.endpoint_v19(provider, parent)?;
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let raw = bounded.api(&endpoint, "GET", None, None, true, &self.cancellation)?;
        self.check_issuer(provider, cancel)?;
        // --slurp wraps the actual first response, even when that page is [].
        // A missing response is not evidence of complete empty enumeration.
        if raw.as_array().is_none_or(Vec::is_empty) {
            return Err(error(
                GithubErrorKind::Json,
                "Conversation detail list lacks its actual first page",
            ));
        }
        let values = GithubProvider::api_list_pages_v19(raw, None)?;
        self.check_issuer(provider, cancel)?;
        Ok(values)
    }
}
