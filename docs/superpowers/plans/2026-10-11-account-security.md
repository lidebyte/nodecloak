# Profile and account security

- Replace profile header with large initials/avatar, username and email; edit avatar with local crop/zoom, save only after confirmation. Align footer account trigger to the left.
- Show actual Google linkage below profile editing. Unlink requires a configured password and password confirmation, plus 2FA when enabled. Prevent account lockout.
- Add password login and set/change password. Argon2id (64 MiB, 3 iterations, 2 lanes), random salt, bounded hashing concurrency, generic login failures and rate limits.
- Add TOTP enrollment, verification before enable, disable, and one-time backup codes. Encrypt TOTP seeds with a domain-separated server key; never return an enrolled seed. Bound and expire setup/login challenges; reject OTP replay.
- Require TOTP or a backup code for password and email-code login. Google OAuth skips application 2FA as requested. Security changes require password confirmation (initial password requires fresh provider/email authentication); changes invalidate other sessions and pending login challenges.
- Extend native account/action models without persisting passwords, TOTP seeds or login challenges in local profile data. QR generation and avatar editing stay local.
- Test login/2FA replay, account linkage, recent-auth requirements and concurrency. Deploy server after checks; restart development app for native command changes.

References: OWASP Password Storage Cheat Sheet; RFC 6238.
