=== AuthDock — Login Security, 2FA, Social Login & Brute Force Protection ===
Contributors: rakibantor
Donate link: https://degird.com
Tags: login security, two-factor authentication, social login, brute force protection, access control
Requires at least: 6.0
Tested up to: 7.1
Requires PHP: 7.4
Stable tag: 2.2.2
License: GPLv2 or later
License URI: https://www.gnu.org/licenses/gpl-2.0.html

All-in-one WordPress authentication: social login, magic links, 2FA, brute force protection, session management & security hardening.

== Description ==

**AuthDock** is one authentication plugin instead of five: social login, magic links, passkeys, two-factor authentication, brute-force protection, audit logging, session control and wp-admin access rules — with a WordPress-native UI, a REST API, and nothing phoning home.

Every feature ships switched off; turning one on is your decision. There is no telemetry, no licence check and no upsell, and with everything off the plugin makes no outbound request at all.

Every form is a shortcode, a block and a theme-overridable template, so the login, registration, password-reset and account screens can live anywhere in your theme.

The feature list is below. What each feature does and does not send to a third party is under **External services**; what is stored, and for how long, is under **Privacy**.

== Installation ==

1. Upload the `authdock` folder to `/wp-content/plugins/`
2. Activate the plugin through the 'Plugins' menu in WordPress
3. Go to **AuthDock** in the admin menu to configure settings
4. Enable the features you want to use

Or install directly from the WordPress plugin repository:

1. Go to **Plugins → Add New** in your WordPress admin
2. Search for "AuthDock"
3. Click **Install Now**, then **Activate**

== Frequently Asked Questions ==

= What does AuthDock actually include? =

These are the screens in the AuthDock menu. The setting-by-setting reference is in `docs/` in the plugin's repository.

**🔑 Sign-in methods**

* **Social login** — Google, Facebook, GitHub, X, Apple, Microsoft, LinkedIn, Discord, Twitch and Slack, with account linking, auto-registration, a default role and email-domain restriction.
* **Magic links** — expiry, rate limiting, allowed roles, custom email copy. Single use, and dead the moment the password changes.
* **Two-factor** — TOTP or emailed codes, per-role enforcement with a grace period, trusted devices, backup codes, encrypted secrets, replay protection.
* **Passkeys** — first or second factor, verified on your own server, with clone detection.
* **Single sign-on** — any OIDC provider with a discovery document, with role mapping.
* **SMS codes** — through a gateway you connect via the `authdock_send_sms` filter.

**🛡️ Protection**

* **Login limiter** — escalating lockouts, auto-blacklisting, CIDR and wildcard allow and deny lists, XML-RPC coverage, trusted proxies, admin alerts.
* **Bot protection** — proof-of-work by default; Turnstile, hCaptcha and reCAPTCHA optional. A provider outage never becomes a login outage.
* **Breached passwords** — checked by k-anonymity, so no password and no full hash leaves your site.
* **Location & network rules** — allow or refuse by country or hosting network, per surface and per role.
* **Hardening** — custom login URL, XML-RPC control, REST API restriction, user-enumeration blocking, a password policy, six security headers.

**👤 Users & access**

* **Access control** — wp-admin restricted by role or IP, with an emergency bypass key.
* **Sessions** — concurrent limits, idle timeouts, per-role cookie lifetimes, remote termination.
* **Account centre** — every session, trusted device, passkey and connected account on one screen the user owns.

**📋 Monitoring**

* **Audit log** — every authentication event in an indexed table; filter by type, date, user or address, export to CSV or JSON, expire on your own schedule.
* **Notifications** — eight admin alerts and six user alerts, throttled, with a test send.
* **Delivery channels** — the same alerts by email, by Telegram, or both, chosen separately for administrators and account holders. Members can connect their own Telegram where a site allows it.
* **Log streaming** — HMAC-signed JSON webhooks, sent after the response has gone out.
* **Security score** — a weighted 0–100 rating, a seven-step setup walkthrough, an optional weekly digest.

**🧰 Recovery & appearance**

* **Recovery** — `wp authdock` on the command line, `AUTHDOCK_SAFE_MODE`, a wp-config.php constant for every control that can lock you out, and Site Health checks.
* **Light, dark or system** — chosen per person, applied on the server, scoped to AuthDock's screens.

= Does AuthDock work with WooCommerce? =

Yes. Social login buttons appear on WooCommerce login and checkout pages when WooCommerce is active. Role-based redirects also work with WooCommerce customer roles.

= Is AuthDock multisite compatible? =

Yes. Each subsite in a WordPress multisite network has independent settings and its own audit log table.

= Will AuthDock slow down my site? =

No. AuthDock uses conditional asset loading — CSS and JavaScript load only where needed. Database queries use proper indexing, and brute force tracking uses lightweight transients instead of database writes.

= What happens when I deactivate the plugin? =

Cron events are cleaned up, but your settings, database tables, and user data are preserved so you can reactivate later without losing configuration.

= What happens when I delete the plugin? =

All plugin data is completely removed: options, user meta (social IDs, 2FA secrets, trusted devices), custom database tables, capabilities, and transients.

= Can I use social login and 2FA together? =

Yes. When a user logs in via social login, they must still complete the 2FA challenge if enabled for their account or role. AuthDock ensures 2FA cannot be bypassed regardless of login method.

= What authenticator apps work with AuthDock 2FA? =

Any TOTP-compatible app works, including Google Authenticator, Authy, Microsoft Authenticator, 1Password, Bitwarden, and FreeOTP.

= What if I get locked out by the custom login URL? =

AuthDock includes a recovery key parameter. Access your login page via `?authdock_recover=YOUR_KEY` to bypass the custom login URL block. The recovery key is set in your security settings.

= Does brute force protection work with Cloudflare or reverse proxies? =

Yes. Configure trusted proxy IPs in the login limiter settings, and AuthDock will correctly read the real client IP from `X-Forwarded-For` headers.

= Can I export my audit logs? =

Yes. Audit logs can be exported in CSV and JSON formats via the REST API or admin UI. CSV exports include formula injection protection for safe spreadsheet use.

= Where can I put the login, registration and account forms? =

Anywhere. Every form is a shortcode, a block and a theme-overridable template.

The shortcodes are `[authdock_login_form]`, `[authdock_register_form]`,
`[authdock_lost_password]`, `[authdock_social_login]`, `[authdock_magic_login]`,
`[authdock_account]` and `[authdock_logout_button]`. There is a block for each of
them with the same options in the sidebar — which parts of the form to show, your own
labels, colours, typography, spacing, and a Card or Plain style — plus a Conditional
Content block, and patterns for a complete login page, registration page and account
page.

A refused sign-in on the front-end form comes back to the same page with the reason,
worded so that it never reveals whether an account exists.

== Screenshots ==

1. **Dashboard** — Overview of authentication activity with live stats and quick feature toggles.
2. **Social Login Settings** — Configure Google, Facebook, GitHub, and X OAuth providers with button style options.
3. **Magic Link Settings** — Configure link expiry, rate limiting, allowed roles, and force-magic mode.
4. **Two-Factor Authentication** — TOTP and email-based 2FA setup with QR code provisioning and backup codes.
5. **Login Protection** — Brute force settings with progressive lockout, IP whitelist/blacklist, and notification options.
6. **Dynamic Redirects** — Role-based login and logout redirect rules with first-login redirect.
7. **Audit Logs** — Searchable, filterable log of all authentication events with CSV/JSON export.
8. **Security Hardening** — Custom login URL, XML-RPC control, security headers, password policies, and user enumeration prevention.
9. **Notifications** — Admin and user notification settings, throttle control, a test send, and a choice of delivery channels: email, Telegram, or both.
10. **Access Control** — wp-admin restriction by role and IP with emergency bypass and admin bar hiding.
11. **Session Management** — Concurrent limits, idle timeout, per-role session duration, and remote termination.
12. **Social Login Buttons** — Clean social login buttons on the WordPress login page.

== Changelog ==

Every earlier release, and the full detail of each one, is in `CHANGELOG.md` in the plugin repository:
https://github.com/FlyToRakib/AuthDock/blob/main/CHANGELOG.md

= 2.2.2 =
**Restores the artwork on this plugin page.** Nothing in the plugin itself changed.

* The banner, the seven screenshots and the original shield icon are back. The 2.2.1 release removed them by mistake while it was being published.

= 2.2.1 =
**A PHP 8.4 fix for audit-log exports, and packaging fixes.** Recommended for sites on PHP 8.4, or that export their audit log.

* **Audit-log CSV exports are strictly RFC 4180.** On PHP 8.4 every export, from the screen and from `wp authdock`, raised a deprecation notice that could land inside the file. The old backslash escaping also let a value holding a backslash before a quote spill into the next column in a spreadsheet.
* The plugin header now carries the full listing title, which is the name this plugin is published under.
* The changelog and upgrade notices on this page fit the limits WordPress.org enforces, so neither is truncated.
* Four database calls in the schema migration and the multisite session list name the Plugin Check sniff in the annotation that already covered them. Both build their SQL from `$wpdb` properties and bind every value.

== Upgrade Notice ==

= 2.2.2 =
No change to the plugin itself: this release restores the banner, screenshots and icon on the WordPress.org plugin page.

= 2.2.1 =
Recommended on PHP 8.4, or if you export your audit log: CSV exports no longer raise deprecation notices and are strictly RFC 4180. Also fixes the plugin page changelog and upgrade notes.

= 2.2.0 =
Security and reliability release from a full audit. Fixes a multisite account-takeover path, password disclosure through bot protection, single-use token races, and a broken custom login URL on subdirectory installs.

= 2.1.8 =
Required for any site using email codes as a second factor. A correct code landed you on the front page instead of the dashboard, and a just-used code left the next minute unable to get a new one. Nothing is reset.

= 2.1.7 =
Recommended if your administrator address is at Gmail, Outlook or Yahoo. Two-factor codes and magic links were being silently discarded by the recipient provider. The sender now changes only where it can be sent as.

= 2.1.6 =
Required for everyone. Several protections reported themselves as working while doing nothing: location rules and bot protection did not stop a sign-in with a correct password, and a suspended account still signed in.

= 2.1.5 =
Required if two-factor after social sign-in or a magic link reports that it cannot find your sign-in session. The challenge no longer depends on a cookie surviving a redirect.

= 2.1.4 =
Required if your site uses Redis or Memcached object caching. Magic links and social sign-ins were stored where a cache flush could erase them, so links died in inboxes and sign-ins failed mid-flow.

= 2.1.3 =
Recommended for everyone, and important if your site runs a page cache. Fixes a session timeout that could log users out the instant they signed in, and stops any authentication screen from being cached — the failures behind "Session expired" on a correct code.

= 2.1.0 =
Recommended for everyone. Six social providers that could not be configured are now reachable, and Trusted Proxies — previously on no screen — now exists and reads the visitor address correctly behind a CDN. Nothing is reset.

= 2.0.0 =
Recommended for everyone: three security fixes, including an unlimited password-guessing route on the screens that turn two-factor off. The admin menu is rebuilt and your old links are redirected. Nothing is reset.

= 1.0.0 =
Initial release of AuthDock. Install to replace multiple security plugins with a single, comprehensive authentication solution.

== External services ==

AuthDock contacts a third-party service only where you have switched on a feature that
needs one, and never otherwise. There is no telemetry, no usage reporting, no licence
check, and no phone-home of any kind. Every feature below is off by default, and with all
of them off the plugin makes no outbound request at all.

Ten features can talk to somebody else's server. Each is listed here with what is sent,
when, and to whom.

= 1. Social Login =

**When:** a provider is configured with your own OAuth credentials and a visitor clicks its
sign-in button.

Clicking a provider's button redirects the visitor's browser to
that provider, carrying the client ID you configured, the redirect URL back to your site, the
requested scopes, and a single-use state and PKCE challenge. Your site then makes one
server-to-server request to exchange the returned code for an access token, and one to read
the profile it grants. AuthDock reads only the account ID, display name, email address,
email-verified flag, and avatar URL from that profile, and sends the provider nothing about
your site or its other users.

If no provider is enabled, none of this runs.

* **Google OAuth** — `accounts.google.com`, `oauth2.googleapis.com`, `www.googleapis.com` — [Terms](https://policies.google.com/terms) | [Privacy](https://policies.google.com/privacy)
* **Facebook Login** — `www.facebook.com`, `graph.facebook.com` — [Terms](https://www.facebook.com/legal/terms) | [Privacy](https://www.facebook.com/privacy/policy/)
* **GitHub OAuth** — `github.com`, `api.github.com` — [Terms](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service) | [Privacy](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement)
* **X (Twitter) OAuth** — `twitter.com`, `api.twitter.com` — [Terms](https://x.com/en/tos) | [Privacy](https://x.com/en/privacy)
* **Sign in with Apple** — `appleid.apple.com` — [Terms](https://www.apple.com/legal/internet-services/) | [Privacy](https://www.apple.com/legal/privacy/)
* **Microsoft / Entra ID** — `login.microsoftonline.com`, `graph.microsoft.com` — [Terms](https://www.microsoft.com/servicesagreement) | [Privacy](https://privacy.microsoft.com/privacystatement)
* **LinkedIn** — `www.linkedin.com`, `api.linkedin.com` — [Terms](https://www.linkedin.com/legal/user-agreement) | [Privacy](https://www.linkedin.com/legal/privacy-policy)
* **Discord** — `discord.com` — [Terms](https://discord.com/terms) | [Privacy](https://discord.com/privacy)
* **Twitch** — `id.twitch.tv`, `api.twitch.tv` — [Terms](https://www.twitch.tv/p/legal/terms-of-service/) | [Privacy](https://www.twitch.tv/p/legal/privacy-notice/)
* **Slack** — `slack.com` — [Terms](https://slack.com/terms-of-service) | [Privacy](https://slack.com/trust/privacy/privacy-policy)

Apple is the one that behaves differently: it returns the account holder's name once, in the
form POST of the very first sign-in and never again, and it offers a relay address ending
`@privaterelay.appleid.com` in place of a real one. AuthDock stores the name on that first
sign-in because there is no second chance, and treats a relay address as a real address.

= 2. Bot Protection, when a CAPTCHA provider is selected =

**When:** Bot Protection is enabled *and* its provider is set to Turnstile, hCaptcha or
reCAPTCHA. The default provider is **Proof of work**, which runs entirely in the visitor's
browser and contacts nobody.

With a CAPTCHA provider selected, the visitor's browser loads that provider's script, and
the provider sets and reads its own cookies under its own domain. When the form is
submitted, your site makes one server-to-server request to the provider's verify endpoint
carrying the secret key you configured, the token the widget produced, and the visitor's IP
address. Nothing about the account being signed into is sent.

* **Cloudflare Turnstile** — `challenges.cloudflare.com` — [Terms](https://www.cloudflare.com/website-terms/) | [Privacy](https://www.cloudflare.com/privacypolicy/)
* **hCaptcha** — `js.hcaptcha.com`, `api.hcaptcha.com` — [Terms](https://www.hcaptcha.com/terms) | [Privacy](https://www.hcaptcha.com/privacy)
* **Google reCAPTCHA** — `www.google.com/recaptcha` — [Terms](https://policies.google.com/terms) | [Privacy](https://policies.google.com/privacy)

= 3. Breached-password detection =

**When:** Breached Passwords is enabled and somebody registers, resets a password, or
changes one on their profile.

Your site SHA-1 hashes the password and sends **the first five hexadecimal characters of
that hash and nothing else** to Have I Been Pwned's range API. The service answers with the
suffixes of every breached hash sharing that prefix, and the comparison happens on your
server. The password does not leave your site, and neither does its full hash — this is the
k-anonymity model the service was designed around. No account name, email address or IP
address is sent. Responses are cached for 24 hours. If the service is unreachable the check
is skipped and the password is allowed, so an outage there never blocks a login here.

* **Have I Been Pwned** — `api.pwnedpasswords.com` — [Terms](https://haveibeenpwned.com/API/v3#AcceptableUse) | [Privacy](https://haveibeenpwned.com/Privacy)

= 4. Location & Network rules, when a database URL is configured =

**When:** Location & Network is enabled *and* you have pasted a database download URL into
its settings. There is no default URL and no built-in provider — with the field empty,
nothing is ever downloaded.

A weekly scheduled task fetches the file from the address you gave it and stores it under
`wp-content/uploads/`. It is a plain download: no visitor data, no site data, and no IP
address is sent, and it happens on a schedule rather than on a request. Look-ups afterwards
read the local file. If your host already reports a country — Cloudflare's `CF-IPCountry`,
or a server-configured `GEOIP_COUNTRY_CODE` — that is used instead and no download is
needed at all.

The address is whatever you configure. If you point it at MaxMind's own endpoint you are
using their service under their terms with your own licence key:

* **MaxMind GeoLite2** *(only if you configure it)* — the host you supply — [Terms](https://www.maxmind.com/en/geolite2/eula) | [Privacy](https://www.maxmind.com/en/privacy-policy)

= 5. Single sign-on (OpenID Connect) =

**When:** you have added an OIDC provider of your own — Okta, Auth0, Keycloak, Entra ID,
Google Workspace, or anything else that publishes a discovery document — and somebody uses
it to sign in. There is no built-in provider and no default address.

Your site fetches the discovery document and the signing keys from the issuer URL you
configured, over HTTPS only, and caches both. Sign-in is the same OAuth exchange as above,
with the identity token's signature verified against those keys on your server. AuthDock
reads the subject, name, email address, email-verified flag and — if you have configured
role mapping — the group claim. Nothing about your site or its other users is sent.

* **Your OIDC provider** *(only the issuer you configure)* — their terms and privacy policy

= 6. Text-message codes =

**When:** SMS is enabled as a second factor *and* you have connected a gateway yourself.

**No SMS provider ships with this plugin.** There is no account, no balance and no
credentials of ours anywhere in it, and none of your messages pass through anything of ours.
Connecting Twilio, Vonage, MessageBird, AWS SNS or a corporate gateway is a filter —
`authdock_send_sms` — and until something is hooked to it, no message is sent and no
outbound request is made. What is sent, and to whom, is entirely determined by the gateway
you connect.

* **Your SMS gateway** *(only the one you connect)* — their terms and privacy policy

= 7. Log streaming (webhooks) =

**When:** Log Streaming is enabled *and* you have entered a webhook URL. There is no default
endpoint.

Security events are POSTed as JSON to the HTTPS address you gave, **after the response has
already been returned to the visitor**, so an endpoint that is slow or gone never delays a
sign-in. Each request carries the event type, the account ID, the event's context, your
site's URL and a timestamp, signed with an HMAC-SHA256 of the body using the secret you set.
Which categories are sent is your choice. Plain HTTP endpoints are refused. A failed
delivery is retried four times over about an hour and then given up on.

* **Your webhook endpoint** *(only the one you configure)* — their terms and privacy policy

= 8. Scheduled log exports =

**Never leaves your server.** A retention copy is written as JSON Lines into a directory
under `wp-content/uploads/` that is closed to the web server. If you give it an email
address, it sends a note saying an export was written and how many entries it holds — never
the log itself, which would put your whole audit trail through a mail server. Listed here
because it writes files, not because it contacts anybody.

= 9. Passkeys =

**Never.** WebAuthn is verified entirely on your own server — the CBOR parsing, the COSE key
handling and the signature checks are all implemented in this plugin. No attestation is sent
anywhere and no third party is contacted. It is listed here only to say so.

= 10. Telegram notifications =

**When:** you connect a Telegram bot on the Delivery Channels screen and choose Telegram as a
delivery channel. Off until you do, and the plugin contacts Telegram at no other time.

**Where:** `https://api.telegram.org` — Telegram Bot API, operated by Telegram Messenger Inc.

**What is sent:** the text of the notification. That is the same content the email version
carries — for example the IP address that was locked out, the username an attempt used, and
the time. Also your bot token, which is how the API authenticates the request.

**When else:** while somebody is connecting their own Telegram account, the site reads the
bot's pending messages so it can match the one-time token they were given. That reads only
messages sent to your own bot, and only during the minute or two somebody is linking.

**Terms:** [Telegram Terms of Service](https://telegram.org/tos) ·
[Telegram Privacy Policy](https://telegram.org/privacy) ·
[Bot API Terms](https://telegram.org/tos/bot-developers)

**What is stored:** your bot token, encrypted; the chat ID alerts are sent to; and, for each
member who connects their own account, that member's Telegram chat ID and handle. Member
linking is off by default. All of it is removed when the plugin is uninstalled, is included
in a personal-data export, and is removed by a personal-data erasure.

== Reporting a security issue ==

This plugin is the front door of the site it runs on, so security reports are welcome and
are handled ahead of feature work.

**Please do not report a vulnerability in the WordPress.org support forum or a public issue
tracker.** Report it privately instead, by opening a security advisory at
https://github.com/FlyToRakib/AuthDock/security/advisories/new — which is private by
construction and can issue a CVE — or by asking for a private channel through the contact
form at https://degird.com/.

You can expect acknowledgement within three working days and an assessment within seven.
Disclosure is coordinated: a fix ships first and the advisory follows. Reporters are
credited by name in the release notes unless they ask not to be.

Findings that require a site administrator to act against their own site are out of scope,
as are the documented break-glass constants behaving as documented — they exist so a
locked-out administrator can get back in, and they require access to wp-config.php.

The full policy, including scope and supported versions, is in SECURITY.md in the source
repository.

== Privacy ==

**What AuthDock stores.** The audit log records authentication events — sign-ins, failed
attempts, lockouts, two-factor changes, password resets — each with the IP address and user
agent of the request. Lockouts are stored against an IP address. Two-factor secrets, backup
codes, magic-link tokens and trusted-device records are stored hashed or encrypted, never in
plain text.

**How long.** The audit log is trimmed on a schedule you set on the Audit Log screen —
30 to 365 days, or unlimited. Lockout records expire on their own. Magic links expire in
minutes, and trusted devices on the duration you configure.

**How much.** The **What to Record** setting on the Audit Log screen chooses between
everything, security events only, and failures only, so a site that does not want a record of
every ordinary sign-in does not have to keep one.

**How much of an address.** The **Addresses** setting on the same screen chooses between
storing the whole address, the network only (`198.51.100.0`), a one-way hash, and nothing at
all. It defaults to the whole address, which is what the plugin has always done. Lockouts and
the allow and block lists are deliberately unaffected — they have to compare a real address
against a range, so anonymising them would switch the protection off. They keep an address
only for as long as the block lasts.

**On uninstall.** Deleting the plugin removes its settings, its tables and its user meta by
default. If you need the audit log to outlive the plugin — as a compliance record often must
— switch on **Keep my data when the plugin is deleted** on the Audit Log screen before
deleting.

**Data subject requests.** AuthDock is registered with WordPress's own privacy tools. Tools →
Export Personal Data returns the account's connected social accounts, known addresses,
remembered devices and last sign-in, plus every audit row belonging to it. Tools → Erase
Personal Data deletes those account records and strips the account, address and browser from
its audit rows, keeping the events themselves — an audit trail a request can empty is not one.
Two-factor secrets and backup codes are kept, and the report says so: removing them would lock
the account out of its own second factor. Deleting the account removes them.

**Suggested policy text.** Settings → Privacy → *Check the policy suggestions* includes an
AuthDock section describing what is collected and for how long, using the retention period
this site actually has configured.

**The full inventory** — every field, where it lives, why, its lifetime, and three gaps named
rather than left to be found — is in `docs/PRIVACY.md` in the plugin's repository.

== Bundled libraries ==

AuthDock ships one third-party library, unmodified and unminified:

* **QRious 4.0.2** (`admin/js/vendor/qrious.js`) — [neocotic/qrious](https://github.com/neocotic/qrious),
  **GPL-3.0**. Draws the enrolment QR code in the browser, so a two-factor secret is never sent
  to a QR image service. It loads on the profile screen only, and only while enrolling.

It is the distribution its author published, copied by `bin/build-vendor.js` and verified
byte for byte by `npm run check:build` — there is no minification step to reproduce, because
the file that runs is the file that was released.

QRious is GPLv3 and AuthDock is GPLv2-or-later, which permits the combination; the plugin as
distributed is therefore effectively GPLv3. Both are GPL and both are compatible with the
WordPress.org guidelines.
