//! Pure native presentation checks. Bounds only: no transport, source or writer.
//! A remote projection never constructs or borrows a local Git read window.
pub(crate) mod sealed {
    pub trait Sealed {}
}
pub trait NativeDiffCheckV3: sealed::Sealed {
    fn check(&self) -> Result<(), String>;
}
impl sealed::Sealed for crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1 {}
impl NativeDiffCheckV3
    for crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1
{
    fn check(&self) -> Result<(), String> {
        crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1::check(self)
    }
}
