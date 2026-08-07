# Security Policy

## Reporting a vulnerability

**Email [security@frostsnap.com](mailto:security@frostsnap.com).**

Please do **not** open a public issue, pull request, or discussion for a suspected vulnerability. Frostsnap
secures real bitcoin, and a public report is visible to everyone — including anyone who might use it — before
a fix exists or users can upgrade.

Useful things to include, as far as you have them:

- What the issue is, and what an attacker gains
- The affected component — app, device firmware (and its version), or coordinator — and the commit or release
- How to reproduce it, or a proof of concept
- Anything you have already published or shared

You do not need a complete analysis. A rough report of something that looks wrong is welcome; we would much
rather investigate a false alarm than miss a real one.

## What happens next

We will acknowledge your report and tell you whether we can reproduce it. If it is a real issue we will work
on a fix, keep you updated, and agree a disclosure timeline with you before anything is published. We will
credit you when we publish, unless you would prefer we did not.

## Scope

Anything that breaks the security model. Concretely, that model is:

1. **A remote attacker gets nothing, even if every device is corrupt — provided the coordinator app is
   honest.** "Remote" means they never physically hold a device or a backup: their only view of the wallet is
   what reaches the chain and the network requests the wallet software makes. So malicious firmware would
   have to smuggle key material out through a signature, a nonce, or some other message that ends up
   on-chain or on the wire. **That is a break, and exactly what we want reported.** Subject to the exception
   below.
2. **An attacker with physical access is covered while they control fewer shares than the threshold.** What
   counts is the number of **distinct signer shares** they control, however they came by them — a device in
   their hands, a backup in their hands, or malicious firmware on a device they never touch. A device that is
   both malicious *and* in their hands is still one share. While that count is **fewer than the signing
   threshold**, no funds should be lost; at or above it those shares can simply sign, and there is nothing
   left to break.
3. **A corrupt coordinator app should not cost you funds while fewer than the threshold of devices are
   corrupt**, provided you verify what you are approving on the device's own screen.

Once an attacker controls the threshold, they can sign whatever they like, so there is nothing left for us
to guarantee.

An attack that breaks any of the three cases above is what we want to hear about.

## Bounty

We pay **1,000,000 satoshi** for an attack that genuinely breaks the model above.

Eligibility is judged when your report reaches us. It must be the **first private report** of something we
do not already know about and have not already fixed — that includes a duplicate of an earlier private
report, and anything already visible in an issue or pull request. It stays eligible only while you follow the
coordinated disclosure above: publishing before we have agreed a timeline forfeits it.

If we believe the attack is deployable and profitable to actually carry out, we may reward more, depending on
severity.

### Exception: malicious backups and ransom

Corrupt devices can always refuse to sign, and can always hand out backup words that do not reconstruct the
share they claim to. Unless backups are verified on independent devices, neither is preventable: nothing in
the protocol can compel a device to cooperate or to tell the truth about a secret only it holds. A set of
corrupt devices can therefore hold a wallet to ransom, and that is a known limitation of the model rather
than a break of it — so it is not eligible for the bounty.
