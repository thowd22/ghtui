//! Actual owned master-fd observations only; no child/process/terminal changes.
use super::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalWitnessV4 {
    input: libc::tcflag_t,
    output: libc::tcflag_t,
    control: libc::tcflag_t,
    local: libc::tcflag_t,
    characters: Vec<libc::cc_t>,
    input_speed: libc::speed_t,
    output_speed: libc::speed_t,
    foreground: libc::pid_t,
    session: libc::pid_t,
}
impl TerminalWitnessV4 {
    pub fn is_raw(&self) -> bool {
        self.local & (libc::ICANON | libc::ECHO) == 0
    }
}
impl Session {
    pub fn actual_terminal_witness_v4(&self) -> Result<TerminalWitnessV4, String> {
        let attrs = super::read_termios(self.master.as_ref())
            .ok_or("actual owned master termios unavailable")?;
        let foreground = self
            .master
            .process_group_leader()
            .ok_or("actual owned terminal foreground unavailable")?;
        // SAFETY: these functions only inspect initialized actual termios and
        // the already captured positive application PID. No signal is sent.
        let session = unsafe { libc::getsid(self.owned_application_pid()) };
        if foreground <= 0 || session <= 0 {
            return Err("actual owned terminal identity unavailable".into());
        }
        Ok(TerminalWitnessV4 {
            input: attrs.c_iflag,
            output: attrs.c_oflag,
            control: attrs.c_cflag,
            local: attrs.c_lflag,
            characters: attrs.c_cc.to_vec(),
            input_speed: unsafe { libc::cfgetispeed(&attrs) },
            output_speed: unsafe { libc::cfgetospeed(&attrs) },
            foreground,
            session,
        })
    }
}
