# Security

savehorse handles encrypted files, destroys files on request, and parses
untrusted input: archives, disk images, executables and torrent files. This page
says what is in scope, what is not, and how to report something.

## Reporting

Email **sohilmomin2000@gmail.com**. Do not open a public issue for a
vulnerability. Include the version, the platform, and the smallest input that
reproduces it.

This is a personal project, not a funded one. I will answer as quickly as I can
and credit you in the fix unless you would rather I did not. There is no bug
bounty.

## What is in scope

- The encryption container in `aegis`: key derivation, nonce handling, and the
  authenticated decryption path.
- Path traversal in `arc extract` and `revive unpack`. Both filter entries that
  would escape the destination; a way around that filter is a vulnerability.
- Memory safety in the parsers. They are Rust, but `forge` reads PE headers at
  offsets taken from the file, and `arc` reads torrent metadata written by
  someone else.
- Anything that causes `shred` to report success without overwriting.

## What is not in scope

- **Physical recovery after `shred`.** Three overwrite passes do not defeat wear
  levelling on flash storage or copy-on-write filesystems. If a block survives
  elsewhere, that is the drive doing its job. Encrypt the volume if that matters.
- **Wrong passphrase.** Decryption failing on a wrong passphrase is correct
  behaviour, not a bug.
- **Signature detection.** `aegis scan` matches a CRC-32 database you supply. It
  is not an antivirus, and CRC-32 is not a security primitive; it is a way to
  recognise files you already know.
- **The `imports` heuristic.** It scans for printable strings ending in `.dll`.
  It does not walk the import table, and does not claim to.
- **Repositories and installers.** Build provenance is whatever GitHub Actions
  gives you. Verify checksums from the release page.

## Design notes worth knowing

Decryption cannot tell a wrong passphrase from a tampered file. AES-GCM reports
one failure for both, deliberately, because distinguishing them tells an attacker
something useful.

The old `aegis` container format (`AEGISENC`) is not supported and will not be.
It derived keys from a linear congruential generator seeded with the file size.
If you have files in that format, decrypt them once with the old binary and
re-encrypt.
