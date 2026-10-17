//! Private producer identity only. No source/read/write/transport accessor and
//! no retained old phase clock or cancellation authority.
use super::*;

pub(crate) struct ThreadPhaseIssuerV16 {
    runner: CommandRunner,
    host: String,
    program: OsString,
    env: BTreeMap<OsString, OsString>,
}
impl ThreadPhaseIssuerV16 {
    pub(crate) fn capture(provider: &GithubProvider) -> GithubResult<Self> {
        use std::os::unix::ffi::OsStrExt;
        let bytes = provider
            .env
            .iter()
            .try_fold(0usize, |sum, (key, value)| {
                sum.checked_add(key.as_bytes().len())?
                    .checked_add(value.as_bytes().len())
            })
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original thread producer metadata exceeds bounds",
                )
            })?;
        if provider.host.len() > 256
            || provider.program.as_bytes().len() > 4096
            || provider.env.len() > 128
            || bytes > 64 * 1024
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original thread producer metadata exceeds bounds",
            ));
        }
        Ok(Self {
            runner: provider.runner.clone(),
            host: provider.host.clone(),
            program: provider.program.clone(),
            env: provider.env.clone(),
        })
    }
    pub(crate) fn matches(&self, provider: &GithubProvider) -> bool {
        self.runner.same_instance(&provider.runner)
            && self.host == provider.host
            && self.program == provider.program
            && self.env == provider.env
    }
}
