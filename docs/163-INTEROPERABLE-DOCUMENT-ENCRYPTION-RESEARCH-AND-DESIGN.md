# 163 — Interoperable document encryption: research and design

**Status:** Research and design. **Nothing in this document is implemented.** **Opened:** 2026-10-04.
**Owner:** unassigned. **Decisions required:** §12.

**No ADR is filed with this document, on purpose.** `SKILL` §8 puts decisions in
`docs/08-ADR-REGISTER.md`, and the seven decisions in §12 are the owner's to make, not this
lane's. When D-1, D-2 and D-5 are answered, the next free register entry is **ADR-064** (the
register runs 001–063, contiguous, and a guard in `webapp/tests/doc_citations.test.mjs` fails on
a gap or a duplicate). What this document *does* record, and what an ADR would otherwise have to
re-establish, is the sourced constraint set that makes the decisions decidable.

### How to read the evidence in this document

Three grades, kept distinct on purpose, because `SKILL` §9 exists for a reason:

1. **Specification text**, quoted with a section number — MS-OFFCRYPTO, MS-CFB, ODF 1.3 part 2,
   ISO/IEC 29500-1. Freely published and read directly. **The exception is ISO 32000, which is
   paywalled and was NOT read** (§4 opens by saying so), so every PDF claim is one grade weaker.
2. **Source code of another product**, cited by file and line — LibreOffice's `oox/source/crypto`
   and `package/source/zippackage`, ONLYOFFICE's `sdkjs`/`web-apps` at a named commit, and the
   `aes`/`cfb`/`cpufeatures` crates' own sources. Read, not inferred.
3. **Measurements run in this tree on 2026-10-04/05**, each with the command that produced it:
   the greps in §1, the wasm engine probes in §1.2, the ODF-manifest experiment in §1.2, the
   crates.io and RustSec queries in §9.3, the MSRV-resolver reproduction in §9.3, and the AES
   throughput table in §7.5. The throughput table is the **weakest** of these — Node rather than
   a browser, on a loaded machine, ±2× on absolutes — and is labelled indicative where it
   appears. **No number in this document may be published** until the experiment that hardens it
   has run (§14).

Where none of the three could answer a question, the question is in §14 as a named experiment
rather than being answered. Nothing here is fenced as recollection, because nothing here is
recollection.

## 0. The requirement, and the thing this is not

The owner's words:

```text
encryption like how document or PDF or document-level encryption works — with other
editors can also decode, not only tied to our platform.
```

Read literally: **a password that must be supplied before the file can be opened at all**,
implemented to the published standards, so that Word, LibreOffice and ONLYOFFICE can decrypt
what we write and we can decrypt what they write. Not a proprietary scheme. Not a platform
service. Not a guard rail.

### 0.1 This is NOT `w:documentProtection`, and conflating them is the mistake we currently make

`w:documentProtection` is a **restriction flag inside the document XML**. The standard itself
says so, in a note the standard's own authors wrote:

> This element specifies the set of document protection restrictions which have been applied to
> the contents of a WordprocessingML document. … *Document protection* is a set of restrictions
> used to prevent unintentional changes to all or part of a WordprocessingML document.
> [*Note*: This protection does not encrypt the document, and malicious applications might
> circumvent its use. **This protection is not intended as a security feature.** *end note*]

— ISO/IEC 29500-1 §17.15.1.29, reproduced in the Open XML SDK reference for
`DocumentFormat.OpenXml.Wordprocessing.DocumentProtection`
([learn.microsoft.com](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.documentprotection),
Remarks, attributed "© ISO/IEC29500: 2008").

Anyone can strip it with `unzip`, edit `word/settings.xml`, `zip`. The hash in
`w:documentProtection/@w:hashValue` protects nothing but the UI affordance.

| | `w:documentProtection` | Package encryption (this document) |
| --- | --- | --- |
| Where it lives | inside `word/settings.xml`, in the clear | the package is not there; there is a different container |
| What the password does | gates a dialog | derives a key without which no bytes are readable |
| Container | ZIP (OPC) | **CFB** (MS-CFB compound file) for OOXML; ZIP for ODF; PDF body objects for PDF |
| Removable without the password | **yes, with `unzip`** | no |
| Standard's own words | "not intended as a security feature" | MS-OFFCRYPTO, ODF §3.4, ISO 32000 §7.6 |
| Our state | shipped; a separate lane is fixing its fidelity | **nothing exists** |

The two are kept in separate sections throughout. Where this document says "encryption" with no
qualifier it means package encryption.

---

## 1. What is measured true about this tree, today

Every row was run in this working tree on 2026-10-04.

| Claim | Command | Result |
| --- | --- | --- |
| No CFB/OLE container support anywhere | `grep -rln "CompoundFile\|cfb\|OLE\|EncryptionInfo" crates/casual-doc-package/src crates/casual-doc-io/src` | exit 1, no output |
| …nor anywhere else under `crates/` | `grep -rln "CompoundFile\|EncryptionInfo\|EncryptedPackage" crates/` | exit 1, no output |
| No crypto crate in the lockfile | `grep -c '^name = "<p>"$' Cargo.lock` for `aes sha2 sha1 hmac pbkdf2 cipher ring rustls aes-gcm cbc sha3 digest subtle zeroize getrandom rand` | **0 for every one** |
| `zip` crypto features are deliberately off | `Cargo.toml:58` | `zip = { version = "=7.2.0", default-features = false, features = ["deflate-flate2-zlib-rs"] }` — and `docs/28` line 25 records the reason: "default `zip` features include encryption and multiple codecs that DOCX does not require" |
| PDF export writes no `/Encrypt` | `grep -rn "Encrypt" crates/casual-doc-pdf/src/*.rs` | no output; `docs/18` line 84 already lists encryption as not covered |

### 1.1 Where the container decision is made

One place, and it is a byte comparison at offset 0:

```rust
// crates/casual-doc-package/src/archive.rs:28
if !bytes.starts_with(LOCAL_FILE_SIGNATURE) {   // b"PK\x03\x04"
    return Err(PackageError::MalformedArchive);
}
```

`casual-doc-package` is the only container substrate; its own doc comment calls itself
"Format-neutral, security-bounded **ZIP** package admission" (`crates/casual-doc-package/src/lib.rs`).
`DocxPackage` (`casual-doc-ooxml`) and `OdtPackage` (`casual-doc-odf`) both layer on it. So a
container decision is not made anywhere today: there is one container, and anything that is not a
ZIP is `MalformedArchive`.

### 1.2 The typed "this is encrypted" reasons that already exist — and are unreachable

Three of them exist:

- `PackageError::EncryptedEntry` — "encrypted ZIP entries are unsupported"
  (`crates/casual-doc-package/src/error.rs:36`, raised at `archive.rs:83` on ZIP
  general-purpose-flag bit 0 and at `package.rs:136`).
- `OoxmlError::EncryptedEntry` — "encrypted DOCX entries are unsupported"
  (`crates/casual-doc-ooxml/src/error.rs:36`, mapped at `:149`).
- `OdfError::EncryptedDocument` — "encrypted ODF documents are unsupported"
  (`crates/casual-doc-odf/src/error.rs:51`), raised when any manifest entry carries
  `manifest:encryption-data` (`crates/casual-doc-odf/src/package.rs:242`).

**All three are unreachable from the product.** `FormatImporter::probe` collapses every adapter
error into `ProbeResult::no_match`:

```rust
// crates/casual-doc-io/src/odt.rs:78
fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
    match OdtPackage::open(request.bytes, self.package_limits) {
        Ok(_) => ProbeResult::definite("odt.odf.text-package"),
        Err(_) => ProbeResult::no_match("odt.odf.not-admitted"),
    }
}
```

`DocxAdapter::probe` (`docx.rs:71`) has the same shape. `FormatRegistry::detect` then finds no
match at all and returns `IoError::UnsupportedFormat { requested: None }`, whose `Display` is
"document format could not be detected" (`crates/casual-doc-io/src/error.rs`).

#### Measured, through the shipped wasm engine

Run against the committed `webapp/pkg` build, in Node, calling the same `open` / `openAs`
exports the browser calls:

| Input | `open` (auto-detect — what a user gets) | `openAs` (explicit format) |
| --- | --- | --- |
| unmodified DOCX fixture | **OPENED** | — |
| a MS-CFB container (the shape of a password-protected `.docx`) | `import document: document format could not be detected` | — |
| the same DOCX with ZIP GP-flag bit 0 set on all 3 central records | `import document: document format could not be detected` | — |
| a repacked ODT, unencrypted (control) | **OPENED** | **OPENED** |
| the same ODT with `manifest:encryption-data` on `content.xml` | `import document: document format could not be detected` | `ODT package admission: **encrypted ODF documents are unsupported**` |

The last row is the finding. The engine **knows**; the path every user takes cannot say so. This
is `SKILL` §9.4 exactly — "built is not reachable" — and it means the first increment of this
programme is not cryptography at all (§11 Phase 0).

It also makes one existing sentence an overstatement by omission (`SKILL` §9.3): `docs/135`
line 313 reads "ODF encrypted packages are detected but rejected with a typed
policy/unsupported result". True of the crate; **not** true of the product, because the typed
result never reaches a reader. §11 Phase 0 closes the gap rather than the sentence.

### 1.3 The seams a password would have to pass through

| Seam | File | Why it blocks |
| --- | --- | --- |
| `ImportRequest` | `crates/casual-doc-io/src/artifact.rs` | `{ bytes, retain_source }`, `#[derive(Clone, Copy, Debug)]`, **not** `#[non_exhaustive]`. No password field, and adding one is the `SKILL` §5a-5 shape exactly: every literal on every branch breaks. |
| `ProbeRequest` | `crates/casual-doc-io/src/registry.rs` | `{ bytes }` only — a probe cannot report "right format, needs a key". |
| `AdapterError` | `crates/casual-doc-io/src/error.rs` | a `String` message. A host cannot branch on "needs a password" versus "corrupt". |
| `IoError` | `crates/casual-doc-io/src/error.rs` | six variants, none of them "password required" or "password wrong". |
| `REFUSAL_CODES` | `webapp/src/host_contract.mjs:101` | `unknown-command`, `capability-withheld`, `unavailable`, `engine-refused`, `threw`, `bad-request`, `timeout`. No password state. |
| `REQUIREMENTS` | `webapp/src/host_contract.mjs:70` | the nine capabilities. A password is **not** one of them (§8.3). |
| wasm façade | `crates/casual-doc-wasm/src/lib.rs:940` `open` / `:959` `open_as` | `(bytes)` and `(bytes, format_id)`. No third argument, and `open` is a one-shot: there is no "try again with this" without a second entry point. |
| the wasm gate | `.github/workflows/ci.yml:191` | `cargo check --workspace --all-features --locked --target wasm32-unknown-unknown`. Any new workspace member must compile for the browser — the constraint ADR-063 was decided under. |

---

## 2. ECMA-376 / MS-OFFCRYPTO — the full chain (Q1)

All section numbers are [MS-OFFCRYPTO], *Office Document Cryptography Structure*, published
revision 14.0 of 2026-02-17.

### 2.1 The structural fact: an encrypted OOXML file is not a ZIP

> When an ECMA-376 document [ECMA-376] is encrypted as specified in [ECMA-376] Part 2 Annex C
> Table C-5 BIT 0, a structured storage utilizing the data spaces construct as specified in
> section 2.1 MUST be used.

— §2.3.4

> The data spaces structure consists of a set of interrelated storages and streams **in an OLE
> compound file as specified in [MS-CFB]**.

— §2.1 (emphasis added)

So the file on disk is an MS-CFB compound file whose first eight bytes are

> **Header Signature (8 bytes):** Identification signature for the compound file structure, and
> MUST be set to the value 0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1.

— [MS-CFB] §2.2

and whose streams include `\EncryptionInfo` (§2.3.4.5 / §2.3.4.10), `\EncryptedPackage` (§2.3.4.4)
and the `\0x06DataSpaces` storage (§2.3.4.1–2.3.4.3). **The extension is still `.docx`.** Therefore:

1. Reading one requires a **CFB reader before any cryptography**.
2. Detecting one requires **sniffing magic bytes**, because the extension lies and the MIME type
   lies too.

`\EncryptedPackage` holds the whole original ZIP:

> **StreamSize (8 bytes):** An unsigned integer that specifies the number of bytes used by data
> encrypted within the **EncryptedData** field, not including the size of the **StreamSize**
> field. Note that the actual size of the **\EncryptedPackage** stream can be larger than this
> value, depending on the block size of the chosen encryption algorithm

— §2.3.4.4

### 2.2 The version field decides the scheme

`\EncryptionInfo` opens with a 4-byte `EncryptionVersionInfo` (`Version`, §2.1.4):

| `vMajor` | `vMinor` | Scheme | Section |
| --- | --- | --- | --- |
| `0x0002`, `0x0003`, `0x0004` | `0x0002` | **Standard** encryption | §2.3.4.5 |
| `0x0003`, `0x0004` | `0x0003` | **Extensible** encryption (external provider) | §2.3.4.6 |
| `0x0004` | `0x0004` | **Agile** encryption | §2.3.4.10 |

Agile additionally mandates `Reserved (4 bytes)` ` == 0x00000040` (§2.3.4.10). Per Appendix A note
`<11>`, Office 2003 wrote `vMajor` `0x0002`, the 2007 system and SP1 wrote `0x0003`, and Office
2007 SP2 / 2010 / 2013 wrote `0x0004`.

Distinct from all three, and **out of scope for this document**: the pre-OOXML binary schemes —
Office Binary Document RC4 CryptoAPI (§2.3.5), Office Binary Document RC4 (§2.3.6) and XOR
Obfuscation (§2.3.7). Those encrypt `.doc`/`.xls`/`.ppt` compound files, and we do not read the
binary formats at all. They matter only as a classification: a CFB we cannot decrypt must be
refused with a *reason*, not mis-parsed.

### 2.3 Standard encryption (§2.3.4.5) — what a reader must still accept

Fixed by the spec table:

- `Flags`: `fCryptoAPI` and `fAES` set, `fDocProps` clear. `SizeExtra` `== 0`.
- `AlgID` ∈ {`0x0000660E` AES-128, `0x0000660F` AES-192, `0x00006610` AES-256}.
- `AlgIDHash` **MUST** be `0x00008004` (SHA-1).
- `KeySize` ∈ {`0x00000080`, `0x000000C0`, `0x00000100`}.
- `ProviderType` SHOULD be `0x00000018`; `CSPName` SHOULD be "Microsoft Enhanced RSA and AES
  Cryptographic Provider" (or the "(Prototype)" variant; Appendix A `<10>` says treat them as
  equivalent).

Key derivation (§2.3.4.7), with H = SHA-1 and a **16-byte** salt:

- `H0 = H(salt + password)`, password as UTF-16LE.
- `Hn = H(iterator + Hn-1)`, `iterator` an unsigned 32-bit value from `0x00000000`, **50,000
  iterations**; last value 49,999. (Fixed — there is no `spinCount`.)
- `Hfinal = H(Hn + block)` with `block == 0x00000000`.
- Then the 0x36/0x5C derivation: `X1 = H(0x36×64 ⊕ Hfinal)`, `X2 = H(0x5C×64 ⊕ Hfinal)`,
  `X3 = X1 ‖ X2`, key = first `cbRequiredKeyLength` bytes of `X3`.

The cipher **MUST use ECB mode** (§2.3.4.7), which is why Appendix A `<9>`/`<12>`/`<13>` recommend
"a block cipher supporting ECB mode". ECB over a whole package is a real weakness — identical
4096-byte… in fact identical 16-byte plaintext blocks encrypt identically, so structure leaks.
This is one reason not to **write** standard encryption (§2.6).

### 2.4 Agile encryption (§2.3.4.10) — the descriptor

After the 8 fixed bytes, `XmlEncryptionDescriptor` is XML in
`http://schemas.microsoft.com/office/2006/encryption`:

```xml
<encryption xmlns="http://schemas.microsoft.com/office/2006/encryption"
            xmlns:p="http://schemas.microsoft.com/office/2006/keyEncryptor/password">
  <keyData saltSize="16" blockSize="16" keyBits="256" hashSize="64"
           cipherAlgorithm="AES" cipherChaining="ChainingModeCBC"
           hashAlgorithm="SHA512" saltValue="…"/>
  <dataIntegrity encryptedHmacKey="…" encryptedHmacValue="…"/>
  <keyEncryptors>
    <keyEncryptor uri="http://schemas.microsoft.com/office/2006/keyEncryptor/password">
      <p:encryptedKey spinCount="100000" saltSize="16" blockSize="16" keyBits="256"
                      hashSize="64" cipherAlgorithm="AES" cipherChaining="ChainingModeCBC"
                      hashAlgorithm="SHA512" saltValue="…"
                      encryptedVerifierHashInput="…" encryptedVerifierHashValue="…"
                      encryptedKeyValue="…"/>
    </keyEncryptor>
  </keyEncryptors>
</encryption>
```

Constraints from the schema and prose, all §2.3.4.10:

- `saltSize` ≥ 1 and ≤ 65,536, and **the decoded `saltValue` length MUST equal `saltSize`**.
- `blockSize` ≥ 2, ≤ 4096, a multiple of 2.
- `keyBits` ≥ 8, a multiple of 8.
- `hashSize` ≥ 1, ≤ 65,536, **and the same number of bytes as the hash algorithm emits**.
- `spinCount` ≤ 10,000,000.
- `cipherAlgorithm` ∈ {AES, RC2, RC4, DES, DESX, 3DES, 3DES_112} — and **"RC4 … MUST NOT be
  used."** RC2/DES/DESX are each annotated "not recommended" (Appendix A `<15>`, `<17>`; `<16>`,
  `<18>`, `<19>` give their key-length quirks). Undefined values MAY be used.
- `cipherChaining` ∈ {`ChainingModeCBC`, `ChainingModeCFB`} — CFB with an **8-bit window**.
- `hashAlgorithm` ∈ {SHA-1, SHA256, SHA384, SHA512, MD5, MD4, MD2, RIPEMD-128, RIPEMD-160,
  WHIRLPOOL}. Appendix A `<20>`/`<21>`: "AES-128 is the default encryption algorithm, and SHA-1
  is the default hashing algorithm if no other algorithms have been configured."
- The `PasswordKeyEncryptor`'s `cipherAlgorithm` and `hashAlgorithm` **MUST** equal
  `keyData`'s. (`blockSize`, `keyBits`, `hashSize`, `saltSize`, `saltValue` are independent.)
- Exactly one `keyEncryptors`, at least one `keyEncryptor`, **exactly one**
  `PasswordKeyEncryptor`; zero or more `CertificateKeyEncryptor`. "The intermediate key MUST be
  the same for all `KeyEncryptor` elements."
- `dataIntegrity` is `minOccurs="0"` and SHOULD be present — see §2.5 for why we must write it.

### 2.5 Agile key derivation, the verifier round trip, and the segment rule

**Key derivation (§2.3.4.11)**, "derived from PKCS #5 … [RFC2898]" but **not** PBKDF2:

- `H0 = H(salt + password)`; password "MUST be provided as an array of Unicode characters".
- `Hn = H(iterator + Hn-1)`, `iterator` unsigned 32-bit from `0x00000000`, incremented
  monotonically until `spinCount` iterations; last value `spinCount - 1`.
- `Hfinal = H(Hn + blockKey)`.
- If `Hfinal` is shorter than `keyBits`, **pad by appending 0x36**; if longer, **truncate**.

A load-bearing ambiguity the spec does not resolve: `iterator` is "an unsigned 32-bit value",
and the spec does not state its byte order. Every interoperable implementation uses
**little-endian**. `LibreOffice`'s agile engine writes the segment block key little-endian
explicitly (`oox/source/crypto/AgileEngine.cxx:492-495`, byte-by-byte `segment & 0xFF`,
`>> 8`, `>> 16`, `>> 24`), which is the same convention. **This must be pinned by a
known-answer test against a file Word produced, not by reading.**

**IV derivation (§2.3.4.12)**, "Initialization vectors are used in all cases":

- with a `blockKey`: `IV = H(KeySalt + blockKey)`;
- without: `IV = KeySalt`;
- then pad with **0x36** to `blockSize`, or truncate to `blockSize`.

**The three `PasswordKeyEncryptor` block keys (§2.3.4.13)** — exact bytes:

| Attribute | `blockKey` |
| --- | --- |
| `encryptedVerifierHashInput` | `fe a7 d2 76 3b 4b 9e 79` |
| `encryptedVerifierHashValue` | `d7 aa 0f 6d 30 61 34 4e` |
| `encryptedKeyValue` | `14 6e 0b e7 ab ac d0 d6` |

**The verifier round trip.** To write: generate a random `saltSize`-byte *verifier input*;
derive a key from (password, `PasswordKeyEncryptor.saltValue`, block key 1); encrypt the verifier
input with `saltValue` as the IV, zero-padded to a `blockSize` multiple → `encryptedVerifierHashInput`.
Hash the verifier input; derive a key with block key 2; encrypt the hash (zero-padded to a
`blockSize` multiple if `hashSize` is not one) → `encryptedVerifierHashValue`. To verify:
decrypt both, hash the recovered verifier input, and compare against the recovered hash value;
equal ⇒ the password is right. **That comparison must be constant-time.**

**The intermediate key.** A random byte array of `keyData.keyBits/8` bytes — generated
independently of the password — encrypted with a key derived from (password, `saltValue`, block
key 3) and `saltValue` as IV → `encryptedKeyValue` (§2.3.4.13). Everything that encrypts the
package uses the *intermediate* key. This is why a password change need not re-encrypt the
package, and why multiple `KeyEncryptor`s can coexist.

**Package encryption (§2.3.4.15)** — the 4096-byte segment rule, quoted whole because it is
small and every byte matters:

> The **EncryptedPackage** stream MUST be encrypted in 4096-byte segments to facilitate nearly
> random access while allowing CBC modes to be used in the encryption process.
>
> The initialization vector for the encryption process MUST be obtained by using the zero-based
> segment number as a **blockKey** and the binary form of the **KeyData.saltValue** as specified
> in section 2.3.4.12. The block number MUST be represented as a 32-bit unsigned integer.
>
> Data blocks MUST then be encrypted by using the initialization vector and the intermediate key
> obtained by decrypting the **encryptedKeyValue** from a **KeyEncryptor** … The final data block
> MUST be padded to the next integral multiple of the **KeyData.blockSize** value. **Any padding
> bytes can be used.** Note that the **StreamSize** field of the **EncryptedPackage** stream
> specifies the number of bytes of unencrypted data …

Consequences for an implementation: the padding is **not** PKCS#7 — the plaintext length comes
from `StreamSize`, and a decryptor must truncate to it rather than interpret padding. Each
4096-byte segment is independently IV'd, so `n` key derivations are *not* needed — one
intermediate key, `ceil(len/4096)` IV hashes. 4096 is **not** `blockSize`; it is the segment
length, and `blockSize` (16 for AES) governs the final padding.

**DataIntegrity (§2.3.4.14)**: decrypt the intermediate key; generate a random `keyData.saltSize`
`Salt`; encrypt it with `keyData.saltValue` + `blockKey = 5f b2 ad 01 0c b9 e1 f6` as IV,
zero-padded → `encryptedHmacKey`. Then HMAC ([RFC2104]) **the entire `\EncryptedPackage` stream,
`StreamSize` field included**, keyed with `Salt`; encrypt that with
`blockKey = a0 67 7f 02 b2 2c 84 33` → `encryptedHmacValue`.

#### A reported deviation from this text that would break interoperability if we follow the spec literally

§2.3.4.14 step 2 says the HMAC key `Salt` is "a random array of bytes … **of the same length as
the value of the `KeyData.saltSize` attribute**" — so with our profile's `saltSize=16`, a
16-byte HMAC key. The `ms-offcrypto-writer` crate's README says Office does something else:

> This does also include one deviation from the standard, which specifies that the HMAC key
> should have a length equal to the salt length in `<keyData>`. However, the reference
> implementation uses an **HMAC key length of 64**.

64 is `hashSize` for SHA-512, not `saltSize`. If that is right, a spec-literal writer produces
an `encryptedHmacKey` Word computes a different HMAC over, and the file either fails Word's
integrity check or ours fails on Word's files — **a defect that only shows up against another
product, never in a round trip against ourselves.**

**This is a third party's README, not a measurement, and it is the single highest-value thing
to settle before writing any agile file.** It is experiment **E12**. Note which way the risk
runs: `hashSize` happens to equal 64 in our chosen profile, so the two readings differ by
exactly the 16-vs-64 byte key length and nothing else — easy to get wrong, easy to test for.

### 2.6 What we should WRITE, and what we must READ

**Write exactly one profile:**

```
vMajor=4 vMinor=4, Reserved=0x40
keyData:  saltSize=16 blockSize=16 keyBits=256 hashSize=64
          cipherAlgorithm=AES cipherChaining=ChainingModeCBC hashAlgorithm=SHA512
PasswordKeyEncryptor: same, plus spinCount=100000
dataIntegrity: PRESENT
```

Three independent reasons, each sourced:

1. **Microsoft's own defaults.** "encrypting Open XML Format files … with the default values —
   AES … with a 256-bit key length, SHA-2, and CBC (cipher block chaining)". The group-policy
   table gives key length 256, chaining CBC, spin count **100000**, salt length **16**
   ([*Cryptography and encryption in Office 2016*](https://learn.microsoft.com/en-us/deployoffice/security/cryptography-and-encryption-in-office)).
   *Caveat, recorded rather than smoothed over:* the same page's policy row reads "**Specify CNG
   hash algorithm** … The default is SHA1", which contradicts its own prose "SHA-2". So
   Microsoft's documentation is self-inconsistent on the hash, and the hash Word actually writes
   must be **measured** (§14 E1), not read.
2. **LibreOffice's reader is a whitelist of four combinations**, and ours must be inside it.
   `AgileEngine::readEncryptionInfo` returns `false` for anything that is not one of
   AES-128/SHA1, AES-128/SHA384, AES-192/SHA384, **AES-256/SHA512**, each with
   `ChainingModeCBC` and `hashSize` equal to that hash's length
   (`oox/source/crypto/AgileEngine.cxx:573-610`). AES-256 with SHA-256 — a combination the
   MS-OFFCRYPTO schema permits — **LibreOffice will refuse to open.** This is the single
   sharpest interoperability constraint found in this research.
3. **It is LibreOffice's own write preset**, so round-tripping is proven by construction:
   `meEncryptionPreset(AgileEncryptionPreset::AES_256_SHA512)` (`AgileEngine.cxx:237`) and
   `setupEncryptionParameters({ 100000, 16, 256, 64, 16, u"AES", u"ChainingModeCBC", u"SHA512" })`
   (`AgileEngine.cxx:739`) — spinCount 100000, saltSize 16, keyBits 256, hashSize 64,
   blockSize 16.

`dataIntegrity` must be present because Appendix A `<22>` says:

> All ECMA-376 documents encrypted by Microsoft Office using agile encryption will have a
> **DataIntegrity** element present. The schema allows for a **DataIntegrity** element to not be
> present because the encryption schema can be used by applications that do not create ECMA-376
> documents.

A file we write without it is detectably not Office-shaped, and LibreOffice calls
`checkDataIntegrity()` after decrypting (`oox/source/crypto/DocumentDecryption.cxx`, `decrypt`).

**Also write the `\0x06DataSpaces` storage.** §2.3.4 makes it mandatory, and LibreOffice reads
`\0x06DataSpaces/DataSpaceMap` to decide *which* decryptor to instantiate
(`DocumentDecryption::readEncryptionInfo` → `"com.sun.star.comp.oox.crypto." + sDataSpaceName`).
It has a fallback — and the comment on it is itself an interoperability datum:

```cpp
// Fallback for documents generated by LO: they sometimes do not have all
// required by MS-OFFCRYPTO specification streams (0x6DataSpaces/DataSpaceMap and others)
SAL_WARN("oox", "Encrypted package does not contain DataSpaceMap");
sDataSpaceName = "StrongEncryptionDataSpace";
```

We should not rely on another product's fallback. Write the real thing.

**Read (accept) a wider set, in this order of priority:**

| Must accept | Why |
| --- | --- |
| Agile, AES-128/192/256 + CBC + SHA-1/256/384/512 | the whole space Word and LibreOffice write |
| Agile, `ChainingModeCFB` | the spec permits it; cheap once CBC works |
| Agile with **no** `dataIntegrity` | `minOccurs="0"`; refusing it would reject conformant files. Report the absence as a compatibility finding, do not fail. |
| Standard, AES-128/192/256 + SHA-1 + ECB, 50,000 iterations | Office 2007-era files, still common; LibreOffice writes AES-128 standard (`Standard2007Engine::setupEncryption` sets `ENCRYPT_ALGO_AES128` + `ENCRYPT_HASH_SHA1`) |
| **Refuse with a reason** | Extensible encryption (§2.3.4.6 — an external COM provider; undecryptable without it); `CertificateKeyEncryptor`-only files; RC4/XOR binary schemes (§2.3.5–2.3.7); RC2/DES/DESX/3DES agile; and any CFB that is not an encrypted OOXML package |

Deliberately **not** implemented, write or read: RC4 (spec: "MUST NOT be used"), RC2, DES,
DESX, 3DES, MD2/MD4/MD5, RIPEMD, WHIRLPOOL, and `CertificateKeyEncryptor`. Each would be a
refusal with a named reason.

---

## 3. ODF encryption (Q2)

A completely different design. Section numbers are OASIS *Open Document Format for Office
Applications (OpenDocument) Version 1.3, Part 2: Packages*
([docs.oasis-open.org](https://docs.oasis-open.org/office/OpenDocument/v1.3/os/part2-packages/OpenDocument-v1.3-os-part2-packages.html)),
§3.4 *Encryption* (§3.4.1 General, §3.4.2 Encryption Process) and the manifest schema in §4.

### 3.1 Shape

- **The package stays a ZIP.** There is no CFB. Detection by magic bytes finds `PK\x03\x04` as
  always; encryption is discovered from the manifest.
- **Per-file encryption**, not per-package: individual ZIP entries are encrypted and each carries
  its own salt and IV.
- `mimetype` and `META-INF/manifest.xml` **shall not** be encrypted (§3.2, §3.4.1). So the
  manifest — which carries every parameter — is readable without the password. (ODF 1.4 adds an
  `encrypted-package` whole-package variant; see §3.4.)
- Each encrypted entry is **deflated first, then encrypted**, and must be flagged `STORED`
  rather than `DEFLATED` in the central directory. **This is why ZIP general-purpose-flag bit 0
  is irrelevant to ODF**: ODF does not use ZIP's own encryption at all, so our
  `PackageError::EncryptedEntry` path (§1.2) is *not* the ODF detector — the manifest is.

### 3.2 The three-stage KDF

1. **Start key.** "The byte sequence representing the password in UTF-8 is used to generate a
   20-byte SHA1 digest" (§3.4.2). `manifest:start-key-generation-name` is `SHA1` by default, or
   `http://www.w3.org/2000/09/xmldsig#sha256`; `manifest:key-size` default 20 octets.
2. **Derived key.** PBKDF2 over HMAC-SHA-1 (§3.4.2). "For each file, a 16-byte salt is generated
   by a random generator"; `manifest:iteration-count` carries the count, **default 1024**;
   `manifest:key-derivation-name` ∈ {`PBKDF2`,
   `urn:oasis:names:tc:opendocument:xmlns:manifest:1.0#pbkdf2`, `PGP`}; `manifest:key-size`
   default 16 octets.
3. **File encryption.** "The derived key is used together with the initialization vector to
   encrypt the file using the Blowfish algorithm in 8-bit cipher feedback (8-bit CFB) mode."
   `manifest:algorithm-name` is `Blowfish CFB` or a URI from W3C *XML Encryption Syntax and
   Processing* §5.2; `manifest:initialisation-vector` is base64.

Integrity: `manifest:encryption-data/@manifest:checksum` with `@manifest:checksum-type` ∈
{`SHA1/1K`, `urn:…:manifest:1.0#sha1-1k`, `urn:…:manifest:1.0#sha256-1k`} — a digest of the
**first 1K** of the compressed-and-encrypted (or plain) data, used as the password check.

### 3.3 LibreOffice's current default is NOT the spec's baseline

Read from LibreOffice `master`, `package/source/zippackage/ZipPackage.cxx`:

| | ODF 1.3 spec baseline | LibreOffice `master` |
| --- | --- | --- |
| Cipher | Blowfish CFB-8 | `AES_CBC_W3C_PADDING`, `AES_GCM_W3C`, Blowfish still accepted (`:1875-1877`) |
| Derived key size | 16 | 32 for both AES modes (`GetDefaultDerivedKeySize`, `:137-150`) |
| Start key | SHA-1, 20 bytes | SHA-1 or **SHA-256** (`:1852`), clamped to SHA-256 on the newer path (`:339`) |
| KDF | PBKDF2/HMAC-SHA-1 | PBKDF2 **or `Argon2id`** (`:1863-1869`, `:1404`) |
| Iterations | default 1024 | **100000**, or **600000** when the package carries an `encrypted-package` entry (`:1398-1401`) |
| Argon2 params | — | `(3, 1<<16, 4)` = t=3, m=64 MiB, p=4 (`:1405`) |
| Checksum | SHA1/1K | SHA1_1K, SHA256_1K, SHA512_1K (`:1893`, `:1923`) |

```cpp
// package/source/zippackage/ZipPackage.cxx:1398-1406
if (m_nKeyDerivationFunctionID == xml::crypto::KDFID::PBKDF2)
{   // if there is only one KDF invocation, increase the safety margin
    oPBKDF2IterationCount.emplace(m_xRootFolder->hasByName(u"encrypted-package"sv) ? 600000 : 100000);
}
else
{
    assert(m_nKeyDerivationFunctionID == xml::crypto::KDFID::Argon2id);
    oArgon2Args.emplace(3, (1<<16), 4);
}
```

The struct constructor's defaults are still the 2005 ones —
`m_nStartKeyGenerationID(DigestID::SHA1)`, `m_oChecksumDigestID(DigestID::SHA1_1K)`,
`m_nKeyDerivationFunctionID(KDFID::PBKDF2)`, `m_nCommonEncryptionID(CipherID::BLOWFISH_CFB_8)`
(`:153-156`) — overridden per document. **Which combination a given LibreOffice version writes
by default for a given ODF version setting is not established by this reading** and is named as
experiment E2 (§14).

**Design consequence.** If we write ODF encryption we should write **AES-256-CBC + SHA-256 start
key + PBKDF2-HMAC-SHA-1/SHA-256 at ≥100,000 iterations**, matching LibreOffice's modern profile
and staying inside the W3C-xmlenc URIs the ODF 1.3 schema already admits. Argon2id and AES-GCM
are LibreOffice-newest and are **not** safe to write for interoperability — an older consumer
will not know the KDF name. Reading them is a separate, later decision (it costs an `argon2`
dependency).

### 3.4 ODF 1.4 and `encrypted-package`

The `hasByName(u"encrypted-package")` branch above is LibreOffice's whole-package ODF encryption
(the manifest then describes one encrypted blob rather than per-file entries). Our ODF export
already writes `manifest:version="1.4"`
(`crates/casual-doc-odf/src/export.rs:5340`), so this variant is in the space we will meet. It
is **not** covered by the ODF 1.3 §3.4 text above and needs its own sourcing before it is read
or written (experiment E3).

---

## 4. PDF encryption (Q3)

### 4.1 Shape

**Sourcing caveat for this whole section.** Unlike MS-OFFCRYPTO (freely published) and ODF
(freely published), **ISO 32000 is a paywalled standard and was not read for this document.**
The section numbers given below (ISO 32000-1 §7.6 *Encryption*, §7.6.3 *Standard Security
Handler*) are the conventional citations and **were not verified against the text**; the
algorithm facts come from the secondary and research sources linked inline. Every PDF claim here
is therefore one grade weaker than the OOXML and ODF claims, and before any PDF encryption is
implemented the standard itself must be obtained (experiment E11).

The trailer's `/Encrypt` dictionary names a security handler; the standard handler is specified in
ISO 32000-1 §7.6.3 / ISO 32000-2 §7.6. `/Filter /Standard`, `/V` (algorithm version), `/R`
(revision), `/O` and `/U` (owner- and user-password validation values), `/P` (permission flags),
and for `/V 5` additionally `/OE`, `/UE`, `/Perms` and `/CF` crypt filters. Strings and streams
are encrypted individually; the document structure stays readable, which is the root of §4.3.

**Two passwords, one key.** The *user* password opens the document with the `/P` restrictions
applied. The *owner* password opens it with full permissions. Both unwrap the **same** file
encryption key.

### 4.2 Which revisions are known-broken — stated plainly

| `/R` | `/V` | Algorithm | Status |
| --- | --- | --- | --- |
| 2 | 1 | RC4, 40-bit, MD5-derived | **Broken.** 40-bit keys are brute-forceable outright; RC4 has keystream biases practical enough that [RFC 7465](https://datatracker.ietf.org/doc/html/rfc7465) prohibits it in TLS. |
| 3 | 2 | RC4, 40–128-bit, MD5-derived | **Broken/weak.** Same cipher, longer key; the MD5-based KDF is unsalted-by-modern-standards and fast. |
| 4 | 4 | RC4-128 or **AESV2** = AES-128-CBC, MD5-derived | **Marginal.** AES fixes the cipher; the KDF is still MD5 and fast, so the password is the only barrier and it is cheap to guess. |
| 5 | 5 | **AESV3** = AES-256, **single SHA-256** of the password | **Broken, and withdrawn.** A single unstretched SHA-256 lets a GPU test password candidates at enormous rates. Published as Adobe Extension Level 3 (Acrobat 9) and **withdrawn by ISO 32000-2** ([iText technical note](https://itextpdf.com/blog/technical-notes/how-solve-unknown-encryption-type-r-6-errors)); analysed at the time by [Sogeti ESEC Lab](http://esec-lab.sogeti.com/posts/2011/09/14/the-undocumented-password-validation-algorithm-of-adobe-reader-x.html). |
| 6 | 5 | AESV3 = AES-256-CBC, **Algorithm 2.B** hardened hash (iterated SHA-256/384/512 interleaved with AES) | **The only revision worth writing.** Required by PDF 2.0; "All R values below 5 are deprecated in the PDF 2.0 specification". |

**So: if we ever write PDF encryption it is `/R 6` `/V 5` AES-256 and nothing else.** Publishing a
password feature on R2–R5 would be a false security claim (§9).

### 4.3 But even R6 does not make PDF encryption confidential against an attacker who can get the file reopened

This is not an opinion. Müller, Ising, Mladenov, Mainka, Schinzel and Schwenk, *Practical
Decryption exFiltration: Breaking PDF Encryption*, ACM CCS 2019
([paper](https://www.pdf-insecurity.org/download/paper-pdf_encryption-ccs2019.pdf),
[DOI](https://dl.acm.org/doi/10.1145/3319535.3354214)) showed two classes:

- **Direct exfiltration.** PDF permits *partially* encrypted documents, so an attacker can wrap
  the encrypted part in attacker-controlled unencrypted content whose actions post the plaintext
  out when the victim opens it. **23 of 27 viewers tested (85%) were vulnerable.**
- **CBC gadgets.** The standard handler uses AES-CBC with **no integrity protection over the
  document**, so ciphertext is malleable: existing plaintext can be modified and whole new
  encrypted objects constructed. **All 27 viewers were vulnerable.**

The honest statement is therefore: PDF `/R 6` resists *offline password guessing* well. It does
**not** give integrity, and it does not protect a document an attacker can modify and hand back.

### 4.4 Permission flags are not security

`/P` is "a polite request, not a lock" — enforced only by cooperating viewers. A tool that
ignores the bits extracts the content. In PDF 2.0, `/Perms` is encrypted and `/R 6` lets a
*conforming* reader detect tampering with the bits, but nothing compels a reader to honour them.
We must never describe `/P` as protection. (We already do not: we write no `/Encrypt` at all.)

### 4.5 We are an export-only PDF producer, which changes the question

`formats::PDF` is documented "Export only: the engine writes PDF and never reads it"
(`crates/casual-doc-io/src/format.rs`). So PDF encryption for us is a **writer-only** feature:
"Export as PDF, with a password". It needs AES-256-CBC, SHA-256/384/512 and a CSPRNG — no CFB, no
PDF *reader*. That makes it the cheapest of the three formats to add **and** the one with the
weakest honest claim. Those two facts pull in opposite directions and the ordering is an owner
decision (§12 D-1).

---

## 5. Interoperability, as far as it can be sourced (Q4)

**Read** = can open an encrypted file given the password. **Write** = can produce one.

### 5.1 OOXML package encryption (MS-OFFCRYPTO)

| Product | Read | Write | Evidence |
| --- | --- | --- | --- |
| Microsoft Word | **yes** | **yes** | MS-OFFCRYPTO Appendix A lists Office 97 → Office LTSC 2024; the Office encryption settings page documents the UI and the group policies that change the algorithm |
| LibreOffice Writer | **yes** | **yes** | `oox/source/crypto/`: `AgileEngine` (read + write, 4 accepted combos, 3 write presets), `Standard2007Engine` (read + write, AES-128/SHA-1 only), `DocumentDecryption`/`DocumentEncryption` drive them over an `oox::ole::OleStorage` CFB |
| ONLYOFFICE **web client** | **no** | **no** | §6 — measured in their source; the password is sent to the server |
| ONLYOFFICE Docs **server** | **unestablished** | **unestablished** | The web client sends the password to the server and gets back a decrypted `Editor.bin` (§6.2), so *something* server-side decrypts; whether that is `x2t` and whether it can also *write* an encrypted package is not established from the two repositories checked out here. Experiment E4. **Do not publish an inference from §6.2 as a fact about their server.** |
| Google Docs | **yes**, with a caveat | **no** | [Google Workspace Updates, 2026-01-14](https://workspaceupdates.googleblog.com/2026/01/edit-password-protected-office-files-google-drive.html) |
| **opendoc, today** | **no** | **no** | §1 |

**Google Docs is the interesting case, and the answer is now documented first-party.** Before
January 2026 Drive could *preview* password-protected Office files
([2017 announcement](https://workspaceupdates.googleblog.com/2017/02/preview-password-protected-files-in-google-drive/)).
As of 2026-01-14:

> Selecting "Preview" opens the document in read-only Preview mode without removing the password.
> New with this launch is the option to **Edit**, which will open the file for **editing** in
> Docs/Sheets/Slides and **remove the password from the file**.

So Google reads it and, on edit, **drops the encryption**. It does not write encrypted Office
files. That is the same shape as ONLYOFFICE's warning (§6.3) and it is the behaviour we must
**not** copy silently (§8.4).

### 5.2 ODF package encryption

| Product | Read | Write | Evidence |
| --- | --- | --- | --- |
| LibreOffice | **yes** | **yes** | `package/source/zippackage/ZipPackage.cxx` (§3.3) |
| Microsoft Word | **no** | **no** | Word's ODF filter is for plain ODT. *Unsourced beyond absence* — named as experiment E5 rather than asserted. |
| ONLYOFFICE web client | **no** | **no** | §6 — no crypto primitives at all, for any format |
| Google Docs | **unestablished** | **unestablished** | the 2026 announcement names "Microsoft Office files"; it does not mention ODF. Experiment E6. |
| **opendoc, today** | **no** | **no** | §1 |

### 5.3 PDF encryption

| Product | Read | Write | Evidence |
| --- | --- | --- | --- |
| Adobe Acrobat / Reader | yes, R2–R6 | yes | ISO 32000-2 is Adobe's own lineage |
| LibreOffice (PDF export) | n/a | **yes, including AES-256** | LibreOffice 25.8 release coverage names "PDF 2.0 export, AES-256 encryption and PDF/A-4 support" ([AlternativeTo summary](https://alternativeto.net/news/2025/8/libreoffice-25-8-launches-with-pdf-2-0-export-aes-256-encryption-and-pdf-a-4-support)) — **secondary source; verify against LibreOffice's own release notes before publishing (E7)** |
| Word | n/a | yes (Acrobat-compatible) | *unsourced* |
| Browser viewers (`pdf.js` etc.) | generally yes R2–R6 | n/a | *unsourced* |
| **opendoc, today** | n/a (export only) | **no** | §1 |

### 5.4 Named experiments, because the table above has holes

See §14. The matrix rows marked "unestablished" or "unsourced" **must not be published** on
`webapp/fidelity.html` or anywhere else until the corresponding experiment has run. `SKILL` §9.3:
absence from a support matrix is an overstatement by omission — so the published version must
enumerate families, including the ones we could not test.

---

## 6. What ONLYOFFICE actually does (Q5)

Read from `/Users/sachin/Desktop/melp/reference/sdkjs` @ `72b0421c0bbf9d01eed9cf14834ae47eb2df1b50`
and `/Users/sachin/Desktop/melp/reference/web-apps` @ `9c0ca538c3b211052347df09d2a4d6781f023403`
(release 9.4.0). Every claim below was re-verified directly, not taken on report.

### 6.1 The web client has no cipher at all

```
$ grep -rn --include='*.js' -E "\bAES\b|pbkdf2|PBKDF2|EncryptionInfo|EncryptedPackage|\
DataSpaces|encryptedVerifierHash|keyEncryptor|crypto\.subtle" . | grep -v "/vendor/"
   (no output)
```

```
$ strings common/hash/hash/engine.wasm | grep -icE "aes|rijndael|cbc"
0
```

Their WASM crypto engine (`common/hash/hash/engine.wasm`, 65 KB, exports `_hash`/`_hash2`) is a
**hash engine only**. `engine.js:649` calls `hashOffice(password, salt, spinCount, alg)` →
`Module["_hash2"]` — no IV, no key size, no cipher id, output `HashSizes[alg]` bytes. There is no
block cipher, no KDF beyond iterated hashing, and no CFB/compound-file reader anywhere
(`OLE` hits in sdkjs are JSDoc for OLE *drawing objects*). The only WebCrypto use is
`crypto.getRandomValues` in `common/random.js:51`.

**So the ONLYOFFICE web client is structurally incapable of opening or writing an encrypted
package** — and our memory note that their web client has no WASM `x2t` is what makes this
decisive: there is nothing else in the browser that could do it.

### 6.2 The password is sent to the server

`sdkjs/word/api.js:3006-3019`, verified verbatim in this tree:

```js
case c_oAscAdvancedOptionsID.DRM:
    this.currentPassword = option.asc_getPassword();
    var v = {
        "id": this.documentId,
        "userid": this.documentUserId,
        "format": this.documentFormat,
        "c": "reopen",
        "title": this.documentTitle,
        "password": option.asc_getPassword(),
        "lcid": this.asc_getLocaleLCID(),
        "nobase64": true
    };
    sendCommand(this, null, v);
```

`sendCommand` (`common/editorscommon.js:960`) sends it over the co-authoring WebSocket, or
HTTP-POSTs it with the JSON in the query string. The browser then receives `Editor.bin` —
already decrypted, already converted (`common/apiBase.js:2222`). Setting a password is the mirror
image (`common/apiBase.js:4400`, verified): `{"c": 'setpassword', "id": …, "password": password}`,
and `asc_resetPassword()` is `asc_setCurrentPassword("")`.

Gating is a **licence flag**: `asc_isProtectionSupport()` returns
`!OfflineApp && this.isProtectionSupport`, where `isProtectionSupport =
this.licenseResult['protectionSupport']` (`common/apiBase.js:1884`, `:4092`).

### 6.3 Their own UI admits the encryption is destroyed on open

`web-apps/apps/common/main/lib/view/OpenDialog.js:831`, verified:

```js
txtProtected: 'Once you enter the password and open the file, the current password to the file will be reset.',
```

Rendered as the default warning body of the password dialog (`OpenDialog.js:81`).

### 6.4 `documentProtection` — the restriction hash, and it IS fully client-side

Confirmed as the restriction hash, not encryption:

- `common/editorscommon.js:14917`:
  `function generateHashParams() { return {spinCount: 100000, saltValue: AscCommon.randomBytes(16).base64()}; }`
- `word/api.js:14434`: `if (!alg) alg = AscCommon.c_oSerCryptAlgorithmSid.SHA_512;`
- `word/api.js:14356-14484` `asc_setDocumentProtection` writes `props.saltValue`,
  `props.spinCount`, `props.cryptAlgorithmSid`, `props.hashValue`, `props.enforcement` onto the
  model and calls `oDocument.SetProtection(props)` — **the document payload is never touched**.
- Model: `word/Editor/DocumentProtection.js:64-91` `CDocProtect`, the exact ECMA-376
  `w:documentProtection` attribute set.
- Serialised as `c_oSer_SettingsType.DocumentProtection` (`word/Editor/Serialize2.js:7267`).
- Unlock compares hashes **client-side** (`word/api.js:14450`) — only possible because the
  content is plaintext.
- Their unlock dialog uses `maxPasswordLength: 15`
  (`web-apps/apps/documenteditor/main/app/controller/DocProtection.js:154`) — the Word
  *restriction* password limit, not an encryption-password limit.

Excel's `sheetProtection`/`workbookProtection`/`protectedRange` use the same primitive
(`cell/model/WorkbookProtection.js:522`, `:833`, `:1274`). They also carry the legacy 16-bit XOR
transforms (`common/editorscommon.js:15926` `prepareWordPassword`,
`cell/model/WorkbookProtection.js:132` `getPasswordHash`).

### 6.5 Their E2EE ("private rooms") is a different thing, and is not file-interoperable

`CEncryptionData` (`common/editorscommon.js:12365-12810`) encrypts the **co-authoring change
stream** (`common/docscoapi.js:1006`, `:1193`) and **images**, not the file format. It does no
crypto itself — it relays to a desktop plugin (`common/plugins.js:462` `sendToEncryption`), is
hard-gated on `window["AscDesktopEditor"]` (`:12399-12420`), and its wire format is a sentinel
blob: `this.cryptoPrefix = AscDesktopEditor.GetEncryptedHeader() || "ENCRYPTED;"` (`:12380`).
The file is built by the closed native binary
(`common/apiBase_plugins.js:631` `buildCryptedStart`). Word cannot read it. The browser cannot
produce it.

### 6.6 What this means for us

**This is a genuine, structural competitive opening — the same shape as the three advantages in
`SKILL` §1.** ONLYOFFICE cannot open a password-protected document without a server, and when it
does open one it tells the user the password is gone. A local-first editor that decrypts in the
browser and **re-encrypts on save with the same password** would be doing something their
architecture cannot do at all. It is the strongest product argument in this document.

Unestablished: whether the ONLYOFFICE *server* writes an encrypted package in response to
`"c": 'setpassword'`, or merely stores the password out of band. `"setpassword"` is handled in the
`ONLYOFFICE/server` repository, which is not checked out here. Experiment E4.

---

## 7. Where this sits in our architecture (Q6)

### 7.1 Who owns a CFB reader/writer

**A new crate, `casual-doc-cfb`.** Not `casual-doc-package` and not `casual-doc-ooxml`.

Reasons, in order:

1. `casual-doc-package` is *documented* as the ZIP substrate — "This crate validates only the ZIP
   container" — and its whole value is being one small, fuzzed, `forbid(unsafe_code)`,
   limit-enforcing admission boundary (1,034 lines). A CFB reader is a second container with a
   different threat surface (a sector FAT is a linked list, so cycles and overlaps are the
   hazard, not path traversal). Two containers in one crate dilutes both.
2. `casual-doc-ooxml` is the OPC *profile*; CFB is also how `.doc` and IRM files are shaped, so
   the container is not OOXML-specific.
3. It keeps the dependency blast radius tight: `casual-doc-cfb` needs no crypto, and
   `casual-doc-crypto` needs no XML.

**The established pattern, named before designing** (`SKILL` §8): this is a **virtual filesystem
over a block device with a FAT** — sectors, a File Allocation Table, a directory as a red-black
tree, and a mini-stream for small entries. The prior art is FAT12/16. The bounded-admission
pattern to copy is our own `BoundedPackage`: enumerate the directory once under limits, build a
name→entry map, refuse cycles/overlaps/duplicates up front, then serve reads. The limits that
matter are different from ZIP's: maximum directory entries, maximum sector-chain length (a FAT
cycle is an infinite read), maximum total stream bytes, and the mini-FAT equivalents.

So, **two** new crates, and one existing crate changed, in dependency order:

```
casual-doc-cfb      container only, no crypto      (MS-CFB read; write in a later phase)
casual-doc-crypto   primitives + the three schemes (MS-OFFCRYPTO, ODF, PDF handler)
  └── depends on casual-doc-cfb for the OOXML scheme
casual-doc-io       gains password-aware detection and dispatch
```

### 7.2 Format detection when the extension lies

The current contract already has the right *shape* — `ProbeConfidence::{NoMatch, Possible,
Definite}` with hints only breaking `Possible` ties — and the right **choke point**
(`FormatRegistry::detect`, `SKILL`/ADR-030 invariant I1). Three changes:

1. **A parallel channel for "this is my format and it is locked."** Without it, every locked
   file is "could not be detected" (§1.2, measured).

   The tempting change — a fourth `ProbeConfidence` variant — has a trap. `ProbeConfidence`
   derives `Ord` and `detect` selects by `.max()`, so a new variant's *position* in the enum
   silently becomes a selection rule. Sorted above `Definite`, a locked candidate would beat a
   genuine definite match; sorted below, a locked candidate loses to any `Possible` match and
   the user gets the wrong format's error. Locked-ness is **not** a degree of confidence; it is
   a second axis.

   So the right shape is a field, not a variant: `ProbeResult` gains
   `locked: Option<LockedScheme>` alongside the existing `confidence` and `evidence`, a probe
   that recognises its own encrypted container returns `Definite` **with** `locked` set, and
   `detect` selects exactly as it does today and then hands `import` a reason instead of the
   bytes.

   Measured: `grep -rn "ProbeResult *{" crates/` finds the type definition and six `fn probe`
   signatures and **no struct literal anywhere** — all 21 construction sites go through
   `ProbeResult::{no_match, possible, definite}`. So the field is additive for every adapter in
   the tree. `ProbeResult` is `pub` with public fields, though, so a *host's* adapter could be
   constructing it literally; the field addition should therefore come with
   `#[non_exhaustive]`, alongside the same change to `ImportRequest` in §7.3.
2. **Sniff the CFB magic before anything else.** Eight bytes at offset 0. A CFB is *not* a ZIP,
   so no existing probe can claim it; and because `D0 CF 11 E0` is unambiguous, a CFB probe can
   return `Definite` with no hint needed. It should then look inside: `\EncryptionInfo` +
   `\EncryptedPackage` ⇒ encrypted OOXML; anything else ⇒ refuse with a reason that says what it
   found (an old `.doc`, an IRM file, an Excel workbook). **Never trust the extension**, and
   never trust the MIME type either — `file_name_hint` and `mime_hint` stay what they are now:
   tie-breakers for `Possible`, never evidence.
3. **The probe must stay cheap and bounded.** `detect` probes *every* registered importer, so a
   CFB probe that parses the whole directory on every open of every file is a tax on the common
   path. Read the header, check the signature and the two version words, and only walk the
   directory if the signature matched.

### 7.3 The registry seam when a format needs a password

The constraint that shapes this: **`ImportRequest` is `Clone, Copy` and not `#[non_exhaustive]`**
(§1.3). `SKILL` §5a-5 says a widely-constructed struct gets `#[non_exhaustive]` plus a builder
*before* a field is added, or two green branches merge into a red `main`. So:

- **Step one, on its own, before any crypto:** make `ImportRequest` (and `ExportRequest`)
  `#[non_exhaustive]` with a constructor/builder that defaults the new fields. That is a
  mechanical, separately-verifiable change, and it is the thing that lets the crypto work land
  without a cross-branch collision. It will cost `Copy` (a builder plus `Copy` is fine; a
  non-exhaustive struct cannot be constructed literally outside the crate, which is the point).
- Then `ImportRequest` gains `password: Option<&SecretPassword>` — a newtype that is
  `zeroize`-on-drop and whose `Debug` prints nothing. Never a bare `&str`, because a `&str` ends
  up in a log line.
- `AdapterError` gains a typed **kind** so a host can branch:
  `PasswordRequired`, `PasswordIncorrect`, `UnsupportedEncryption { scheme }`,
  `IntegrityFailed`, plus the existing opaque message. `IoError` gains the matching variants so
  the distinction survives `ImportFailed`. The message must stay content-free (`AdapterError`'s
  existing contract: "does not expose document contents").
- **`PasswordRequired` must be reachable without a password.** The open flow is
  *detect → report locked → chrome asks → retry with the password*, and the first step must not
  need a key. Agile and standard encryption both put all parameters in `\EncryptionInfo`, and ODF
  puts them in the unencrypted manifest, so this is always possible.
- **`PasswordIncorrect` must be distinguishable from `IntegrityFailed`.** The verifier round trip
  (§2.5) answers "wrong password". A verifier that passes while the HMAC fails means the file was
  *tampered with*, and telling a user "wrong password" there would hide an attack.
- **Export** needs the mirror: `ExportRequest` gains the password, and the exporter must be able
  to refuse ("this format cannot be encrypted") rather than silently write a plaintext file. A
  silent drop here is exactly the "no silent data loss" rule in `AGENTS.md`, applied to
  confidentiality.

### 7.4 Host contract and the capability model

**A password is not a capability.** `REQUIREMENTS` (`webapp/src/host_contract.mjs:70`) answers
"may this page do X" — a host-granted permission. A password answers "do you hold the key to
this file", which the host does not grant and cannot withhold. Adding `password` to the nine
would be a category error, and the file's own comment already draws the line (`mutate` is not a
capability because it is a different kind of question).

**But "this document needs a password" is a STATE the chrome must represent**, and it is a state
the editor has never had. Concretely:

1. **A new refusal code.** `REFUSAL_CODES` gains `password-required` and
   `password-incorrect`. These are codes a host branches on; the sentence stays localised in the
   chrome, exactly as the file's comment requires.
2. **A new host event, or a documented `ready` that never arrives.** `HOST_EVENTS` has `ready`,
   `change`, `selection`, `save`, `export`, `refusal`. An embedded editor handed a locked file
   today would emit nothing a host can act on. The honest shape is a `locked` event carrying
   `{ format, scheme }` and no document — and `ready` is then **not** emitted until the document
   actually opens. A host that wants to supply the password itself (a DMS that already holds it)
   can then do so without a dialog.
3. **The chrome needs a real password prompt, designed from Word and Docs first**
   (the editing-standard rule). This is the first place that rule bites: a `window.prompt` would
   be the hyperlink defect all over again. Word's shape is a modal with one obscured field,
   Enter to submit, Escape to cancel-and-close-the-document, a distinct wrong-password state
   that does not close the modal, and no attempt counter. Docs' shape is the same. The prompt
   must be keyboard-operable, screen-reader-operable, localised and themed (`SKILL` §10) and must
   **never** echo the password or put it in the DOM where an extension can read it after the fact.
4. **Never a dead control.** "Protect with a password" must not appear until a build can honour
   it; where a format cannot be encrypted the control ships **disabled with a reason**.
5. **Reachable from ≥2 surfaces** once it exists (`SKILL` §10): File menu and the ribbon's File
   band, at minimum, plus the command palette.

### 7.5 Can a WASM build do AES-256 fast enough, and what would establish it

The honest answer is **not established**, and the shape of the answer is predictable:

- There are three sizes of work, and they are very different.
  - **Key derivation.** 100,000 iterations of SHA-512 over ~64 bytes. This is *small* — a few
    MB of hashing — but it is deliberately serial and cannot be parallelised. It happens once
    per open.
  - **Package decryption.** AES-256-CBC over the whole ZIP. A 10 MB `.docx` is 10 MB of AES plus
    2,560 IV hashes. A 64 MB file (our `MAX_OPEN_BYTES`, `webapp/src/main.js:14882`) is 64 MB.
  - **The HMAC** over the encrypted package: one more pass over the same bytes.
- **The thing that decides it is whether the browser gets hardware AES.** On x86-64 and aarch64
  the `aes` crate uses AES-NI / ARMv8 crypto extensions and runs at GB/s. `wasm32-unknown-unknown`
  has **no AES instruction** — the WebAssembly SIMD proposal provides 128-bit vector ops but no
  AES round instruction — so a Rust wasm build falls back to a constant-time bitsliced software
  implementation, which is roughly an order of magnitude slower. Our existing wasm build is
  `cargo check --target wasm32-unknown-unknown` with no `+simd128`, so even the vectorised
  software path is not currently enabled.
- **The alternative is WebCrypto**, which is the browser's native AES and does get hardware
  acceleration — but it is `async`, lives in JavaScript, and would put the cipher on the far side
  of the wasm boundary. That is a real architectural fork: a pure-Rust engine that works
  identically native/wasm/headless, versus a browser-fast path that makes the engine depend on a
  host-provided primitive (which the host contract would then have to specify). **This is owner
  decision D-5.**

#### An indicative measurement exists, and it moves the answer

A lane of this research built a `cdylib` exporting AES-256-CBC
`encrypt_padded`/`decrypt_padded::<NoPadding>` over a 1 MiB buffer
(`opt-level=3, lto=true, codegen-units=1`) and drove it from Node 22.18.0 — **V8, the same wasm
pipeline Chrome uses** — through plain `WebAssembly.instantiate`, medians of 5–9 reps:

| Build | AES-256-CBC encrypt | AES-256-CBC decrypt |
| --- | ---: | ---: |
| wasm32, V8, default | 41.1 MiB/s | 104.8 MiB/s |
| wasm32, V8, `-C target-feature=+simd128` | 46.6 MiB/s | 132.3 MiB/s |
| wasm32, `--cfg cpubits="64"` | 15.0 MiB/s | 79.1 MiB/s |
| native aarch64, `aes_backend="soft"` (fixslice) | 27.8 MiB/s | 73.4 MiB/s |
| native aarch64, ARMv8 AES intrinsics | 1198 MiB/s | 5114 MiB/s |

**Treat the absolutes as ±2× and the ratios as the result.** The machine was running other
heavy work (load average 47 at one point) and repeats under load gave 16.8/58.4 MiB/s for the
wasm cases; ratios were taken within single runs, so they survive the noise. It is **Node, not a
browser**, and not from a dedicated Worker on an idle machine. One published figure agrees:
`jedisct1/rust-aes-wasm`'s README measures the `aes`+`cbc` combination at **35.49 M/s** for
AES-256-CBC under Wasmtime on an Apple M1 — a different engine, same ballpark. Two independent
measurements agreeing is the strongest evidence available short of E8.

Five things follow, and three of them change the design:

1. **wasm ≈ the native software backend** (41 vs 28, 105 vs 73), which independently confirms
   that wasm is running the fixsliced code and nothing else — the source reading above, measured.
2. **wasm is ~25–30× slower than hardware AES on encrypt and ~35–50× on decrypt.** A big
   multiple, and on absolute numbers that mostly do not matter — see 4.
3. **`+simd128` buys 10–25%, not a backend change.** Worth enabling; not AES acceleration.
   `--cfg cpubits="64"` makes wasm **slower** (V8 does not reward 64-bit bitslicing in wasm32),
   so leave `cpubits` alone.
4. **CBC decrypt is 2.5–3× faster than CBC encrypt in the same build, and this is structural:**
   CBC encryption is serially chained so it can only ever use the one-block path, while CBC
   decryption parallelises and feeds the fixslice batch. **Decryption is our common case** —
   opening a protected `.docx` — so the fast direction is the one we need. At ~105 MiB/s a
   10 MB package decrypts in roughly **0.1 s**, and 100 MB in about a second. Against everything
   else opening a document costs, that is not the bottleneck.
5. **Therefore D-5 probably resolves to pure Rust**, and R5 drops down the risk list. WebCrypto
   would buy an order of magnitude on a cost that is already ~0.1 s for a realistic file, at the
   price of a second cipher implementation and a host-provided primitive in the contract. The
   engineering that pays is **chunking and Worker offload**, not a faster cipher.

**E8 is therefore narrowed, not cancelled.** What is still unmeasured, and still gating:

1. A real **browser** figure (Chrome, Firefox, Safari), from a **dedicated Worker**, on an idle
   machine with the power profile pinned, over {64 KiB, 1 MiB, 16 MiB} × {encrypt, decrypt} ×
   {default, `+simd128`}, discarding the first 2 s as warm-up (Liftoff → TurboFan tiering),
   medians of ≥20 reps.
2. **`crypto.subtle.decrypt('AES-CBC')` in the same page, in the same units** — the WebCrypto
   ceiling. This is the control that actually decides D-5, because it gives the crossover size
   at which offloading bulk AES would be worth a second implementation.
3. **The KDF measured on its own:** 100,000 iterations of SHA-512 with `sha2` in wasm. This is a
   *serial* cost that cannot be chunked or parallelised, and it is what a user experiences as
   "the password dialog hangs". It is not covered by the table above at all.
4. **The acceptance criterion is time-to-interactive, not throughput** (`SKILL` §"Performance is
   a gate"): a 10 MB protected document must open within the budget a 10 MB document already
   has. Decryption and derivation are both O(document), so regardless of the number they run off
   the main thread, show progress, and are cancellable — the rule that already governs opening.

Until E8's browser figures exist, **no performance number may be published** (`SKILL` §9.1), and
the numbers in the table above are indicative, in this document, and not for a public page.

---

## 8. The threat model, stated honestly (Q7)

### 8.1 What package encryption protects against

- **A file at rest in the wrong place.** An attachment on a mail server, a file on a lost laptop,
  a document in a shared folder, a backup. Without the password the bytes are AES-256
  ciphertext and the document is not readable.
- **Casual and semi-determined offline guessing**, in proportion to the password's entropy and
  the `spinCount`. 100,000 iterations of SHA-512 is a meaningful but not enormous cost factor:
  it slows a GPU attacker by ~5 orders of magnitude relative to a single hash, which turns a
  weak password from instant into merely cheap. **A weak password is still a weak password.**
- **Accidental disclosure.** The commonest real case, and the one the feature mostly serves.

### 8.2 What it does not protect against — and these are the ones to say out loud

- **The password is in the browser's memory**, as a JavaScript string before it reaches wasm, and
  JavaScript strings are immutable and not zeroable. Anything with script access to the page —
  a malicious extension, an XSS, a compromised host embedding us in an iframe with a bad CSP —
  can read it. `zeroize` on the Rust side reduces the window; it does not close it.
- **The decrypted package exists in memory**, in full, because the import path takes `&[u8]` of
  the whole file. So does the decrypted document model, the layout, and the rendered pages. A
  heap snapshot, a crash dump, a swapped page, or a `SharedArrayBuffer` leak exposes plaintext.
- **A local-first editor has no server to hold a key.** There is no escrow, no revocation, no
  "disable access", no audit log, and no key rotation that does not mean re-encrypting the file.
  Lose the password and the document is gone — Microsoft ships a `DocRecrypt` tool for exactly
  this and it only works with a pre-provisioned escrow key we have no way to provision.
- **No identity, no authorisation.** Everyone with the password has the same access. Package
  encryption is not access control: it cannot express "Alice may read, Bob may edit".
- **Metadata leaks.** The CFB container reveals that a document is encrypted, its approximate
  size, and the scheme and `spinCount`. ODF leaks more: the *manifest is not encrypted*, so the
  file list, paths and media types of every entry are readable without the password.
- **Integrity is partial.** The agile `dataIntegrity` HMAC covers the encrypted package and is a
  real tamper check — but it is keyed with the *intermediate* key, which anyone with the password
  holds, so it detects tampering by someone **without** the password, not by someone with it.
  And `dataIntegrity` is `minOccurs="0"`, so a file can arrive without one; AES-CBC with no MAC
  is malleable, which is the exact root of the PDF break in §4.3.
- **Collaboration is incompatible, for now.** A document in a room (our "one doc, one room"
  rule) is relayed as operations through a server that must hold the plaintext ops. Package
  encryption protects the *file*; it says nothing about the session. Promising otherwise would be
  false.
- **The chosen-ciphertext / malleability class is not addressed by the format.** We would be
  implementing what the standards specify, and CBC-without-authentication is what they specify.

### 8.3 What we may truthfully claim

Permitted, once the corresponding code exists and the interoperability experiments have run:

- "Open password-protected `.docx` files, in the browser, with no server." (After E1 passes.)
- "Save a `.docx` encrypted to the ECMA-376 agile standard (AES-256-CBC, SHA-512, spinCount
  100000), which Word and LibreOffice open with the same password." (After E1 **and** a
  round-trip test against both.)
- "The password never leaves your browser." (True by construction for us, and the sharpest
  contrast with ONLYOFFICE §6.2 — but only sayable if the build genuinely never posts it, which
  needs a guard, not a promise.)
- "Opening an encrypted file keeps it encrypted when you save." (Only if we implement it; §8.4.)

### 8.4 What would be a FALSE claim

`SKILL` §9 exists because `webapp/fidelity.html` carried fabricated claims twice, and a security
claim is the worst kind to get wrong. These sentences must never be published:

| Forbidden claim | Why it is false |
| --- | --- |
| "End-to-end encrypted" | There is no second end. Package encryption is at-rest encryption of a file. |
| "Zero-knowledge" / "we cannot see your document" | Vacuous for a local-first editor with no server, and it implies a server architecture we do not have. |
| "Secure" / "military-grade" / "unbreakable" | Unquantified. The strength is the password's entropy times the KDF cost, and we control only the second. |
| "Protected" without saying against what | The word that made `w:documentProtection` a product lie. |
| Any throughput or open-time number before E8 | §9.1: a published number is generated from a committed artifact or it is not published. |
| Any "Word/LibreOffice/ONLYOFFICE opens our encrypted files" before a round-trip test | §9.2/§9.4. Reading a spec is not a passing test. |
| "Encryption is preserved on save" while we silently drop it | This is the thing Google Docs and ONLYOFFICE both do, and both at least *warn*. If phase 1 is read-only we must say "opening an encrypted file and saving it writes an **unencrypted** file" in the UI, at the moment of saving, not in a footnote. |
| Listing encryption as supported for a format we only read | §9.3 — enumerate families, not successes. |

**One more, specific to this project:** the support matrix must not say "encrypted ODF packages
are detected and rejected with a typed result" while that result is unreachable (§1.2). Either
make it reachable or change the sentence — and the first is the better fix.

---

## 9. Crypto dependency policy (Q8)

### 9.1 What CI enforces

`deny.toml`, whole file, and two jobs:

```toml
[advisories]
yanked = "deny"
unmaintained = "workspace"

[bans]
multiple-versions = "warn"
wildcards = "deny"
highlight = "simplest-path"

[licenses]
version = 2
confidence-threshold = 0.93
unused-allowed-license = "allow"
allow = [
    "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC",
    "MIT", "NCSA", "Unicode-3.0", "Unlicense", "Zlib",
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"
required-git-spec = "rev"
```

- `dependency-policy` (`.github/workflows/ci.yml:292`) runs `cargo deny --locked check bans
  licenses sources` plus `cargo audit --deny warnings`, for the workspace and for `fuzz/`.
- `security.yml:28` runs `cargo deny --locked check` (advisories included) weekly.
- `repository-policy` (`ci.yml:323`) runs `cargo metadata --locked`, so **`Cargo.lock` must be
  committed and current**.
- The `platform` matrix checks **MSRV 1.88.0** with `--locked`, so a crate whose own
  `rust-version` exceeds 1.88.0 is unusable.
- `wasm` (`ci.yml:191`) runs `cargo check --workspace --all-features --locked --target
  wasm32-unknown-unknown`, so **every new workspace member must compile for the browser** — the
  constraint ADR-063 was decided under, and the reason `getrandom` is the one real hazard here.

There is **no allow-list of permitted crates**, no `bans.deny`, and no cap on dependency count,
so nothing refuses a crypto crate on principle. `licenses.version = 2` with no `deny` and no
`exceptions` means **anything outside those nine licences is an error**. `cargo audit --deny
warnings` means an `unmaintained` or `yanked` advisory on any new crate fails the PR.

### 9.2 The policy, in one sentence

**Do not write our own primitives.** Not AES, not SHA, not HMAC, not PBKDF2, not a
constant-time comparison. Every primitive comes from a maintained crate under a permitted
licence. The code we write is the *format*: the CFB container, the `EncryptionInfo` descriptor,
the block-key derivation, the segment loop, the manifest parsing. Those are bugs-in-plumbing
risks; a hand-rolled cipher is a bugs-in-cryptography risk, and we are not qualified to take it.

### 9.3 Crate recommendations

All version, licence, MSRV, download and date figures below were read from the crates.io API on
2026-10-04 and each is checked against the five gates in §9.1. Advisory counts were read from
`rustsec/advisory-db` `crates/<name>/`.

| Crate | Version | Licence | Declared MSRV | Advisories | Phase | Notes |
| --- | --- | --- | ---: | --- | --- | --- |
| `cfb` | 0.15.0 | MIT | **1.74** | none | 0 (read), 2 (write) | The CFB container. Read **and write**. 68.3M downloads, released 2026-09-18, by Matthew D. Steele (`github.com/mdsteele/rust-cfb`). |
| `aes` | **=0.9.2** | MIT OR Apache-2.0 | 1.85 | none | 1 | **0.9.3 declares `rust-version = 1.89` and fails our MSRV gate** — pin to `=0.9.2`. RustCrypto. 340M downloads. |
| `cbc` | 0.2.x | MIT OR Apache-2.0 | 1.85 | none | 1 | CBC mode. RustCrypto `block-modes`. Pairs with `cipher` 0.5 / `aes` 0.9. |
| `sha1` | 0.11.x | MIT OR Apache-2.0 | 1.85 | none | 1 | Standard encryption and the ODF start key. RustCrypto `hashes`. |
| `sha2` | 0.11.x | MIT OR Apache-2.0 | 1.85 | 1, not applicable | 1 | SHA-256/384/512. The only advisory is RUSTSEC-2021-0100 — a buggy AVX2 backend in **0.9.7 only**, patched ≥ 0.9.8. 1.01B downloads. |
| `hmac` | 0.13.x | MIT OR Apache-2.0 | 1.85 | none | 1 | The agile `dataIntegrity` HMAC and ODF's PBKDF2. RustCrypto `MACs`. |
| `subtle` | 2.6.1 | **BSD-3-Clause** (permitted) | — | none | 1 | Constant-time equality for the verifier compare. `dalek-cryptography`. 741M downloads. |
| `zeroize` | 1.9.x | Apache-2.0 OR MIT | 1.85 | none | 1 | Wipes the password, the derived keys and the intermediate key. RustCrypto `utils`. |
| `cfb-mode` | 0.9.x | MIT OR Apache-2.0 | 1.85 | none | 1 (agile CFB), 3 (ODF Blowfish) | **Name collision warning: this is the CFB cipher *mode*, nothing to do with the `cfb` container crate above.** Both would be in the same `Cargo.toml`. |
| `getrandom` | 0.4.x | MIT OR Apache-2.0 | 1.85 | none | **2 only** | The CSPRNG. 2.16B downloads. Not needed to *read* — see §9.4. Its wasm configuration is the open item below. |
| `pbkdf2` | 0.13.x | MIT OR Apache-2.0 | 1.85 | none | 3 | ODF's KDF. RustCrypto `password-hashes`. |
| `blowfish` | 0.10.x | MIT OR Apache-2.0 | 1.85 | none | 3 | ODF 1.3's baseline cipher, read-only. |
| `argon2` | 0.6.x | MIT OR Apache-2.0 | 1.85 | none | later, only if we read LibreOffice's newest ODF (§3.3) | RustCrypto `password-hashes`. |
| `rc4` | — | MIT OR Apache-2.0 | 1.85 | none | **never** | Listed to say no. MS-OFFCRYPTO §2.3.4.10: RC4 "MUST NOT be used"; we do not read the binary formats. |

Everything in that table is licence-clean against the nine-licence allow list, comes from
crates.io, and declares an MSRV at or below 1.88.0 — with the one exception called out.

#### Three MSRV/wasm hazards — and the first one needs no pin, which was worth measuring

1. **`aes` 0.9.3 declares MSRV 1.89 and `uuid` 1.27.0 declares 1.89.0, and *neither needs an
   `=` pin*.** The workspace already sets `resolver = "3"` (`Cargo.toml:26`) and
   `rust-version = "1.88.0"` (`:31`), which turns on Cargo's MSRV-aware resolver. Reproduced in
   the scratchpad with a throwaway crate depending on `aes = "0.9"`, `cbc`, `sha2`, `hmac` and
   `cfb`:

   ```text
        Locking 40 packages to latest Rust 1.88.0 compatible versions
          Adding aes v0.9.2 (available: v0.9.3, requires Rust 1.89)
          Adding uuid v1.26.1 (available: v1.27.0, requires Rust 1.89.0)
   ```

   So Cargo holds both back by itself, including the transitive `uuid` that arrives via `cfb`.
   **An earlier draft of this document recommended `aes = "=0.9.2"`; that was overcautious and
   is withdrawn** — an `=` pin would also freeze out patch releases, which is the opposite of
   what you want on a cipher. The two conditions that must hold: the new crates inherit
   `rust-version.workspace = true`, and nothing runs a bare `cargo update` on a newer toolchain
   without the MSRV resolver in effect. The `--locked` CI commands are the backstop.
2. **`cfb`'s other transitive deps are clean:** `fnv` (Apache-2.0 / MIT) and `web-time`
   (MIT OR Apache-2.0). `web-time` being there is itself the signal that the crate was made
   wasm-aware deliberately — and it has a sharp edge, see §9.3's prior-art note on
   `msoffice-crypto`.
3. **Never add `cpufeatures` as an unconditional dependency: on wasm it does not degrade, it
   refuses to compile.** Verified in the vendored source,
   `cpufeatures-0.3.1/src/lib.rs:26-31`:

   ```rust
   #[cfg(not(any(
       target_arch = "aarch64",
       target_arch = "loongarch64",
       target_arch = "x86",
       target_arch = "x86_64"
   )))]
   compile_error!("This crate works only on `aarch64`, `loongarch64`, `x86`, and `x86-64` targets.");
   ```

   The RustCrypto crates that use it gate it per-target in their own `Cargo.toml`, so it never
   reaches a wasm build through them. A hand-written `cpufeatures` line in our crate would break
   the `wasm` CI job outright.

#### `getrandom` on `wasm32-unknown-unknown` — the answer, and why the library should not ask for it

Without configuration a wasm32 build **hard-fails at compile time**
(`getrandom-0.4.3/src/backends.rs`: the wasm arm is a `compile_error!` unless the feature is on).
What is needed has changed three times, which is why this is written down:

| `getrandom` | What `wasm32-unknown-unknown` needs |
| --- | --- |
| 0.2.x | `features = ["js"]` |
| **0.3.0 – 0.3.3** | **both** `features = ["wasm_js"]` **and** `RUSTFLAGS='--cfg getrandom_backend="wasm_js"'` — the feature alone does nothing |
| 0.3.4+ | the feature alone (its `compile_error!` text is stale and still claims otherwise) |
| **0.4.x** | the feature alone; `getrandom_backend` no longer accepts `"wasm_js"` at all |

So on 0.4.x it is `getrandom = { version = "0.4", features = ["wasm_js"] }` and no `RUSTFLAGS`.

**But the library should not be the crate that enables it.** `getrandom`'s own README:

> We strongly recommend against enabling this feature in libraries (except for tests) since it
> is known to break non-Web WASM builds and further since the usage of `wasm-bindgen` causes
> significant bloat to `Cargo.lock` (on all targets).

We happen to fall inside its stated exception, because `cfb` already pulls `web-time` →
`js-sys`/`wasm-bindgen` on wasm32-unknown-unknown. That makes it *defensible*, not *right*. Two
better shapes, and the second is the one to prefer:

- **Enable `wasm_js` at the leaf** (`casual-doc-wasm`), never in `casual-doc-crypto`, so a future
  non-JS wasm consumer of the engine is not poisoned by a transitive `wasm-bindgen`.
- **Better: take the entropy from the host and keep `getrandom` out of the library's API
  entirely.** `casual-doc-crypto`'s writing functions accept the random bytes they need rather
  than fetching them — the salts, the verifier input, the intermediate key, the HMAC key. The
  browser leaf supplies `crypto.getRandomValues` through the seam it already has; a native host
  supplies `getrandom`; a test supplies a fixed vector, **which is the only way a known-answer
  test of the writer is possible at all** (R2). `getrandom` also offers `custom`/`extern_impl`
  backends for exactly this. Note the consequence for the host contract: entropy becomes a host
  responsibility, which is consistent with `AGENTS.md` ("host applications own policy") and must
  be stated there rather than assumed.

Also: **a decrypt-only build needs no CSPRNG at all**, which is the whole reason phase 1 can
avoid this question. Watch `argon2`, whose default features include `getrandom` — take it as
`argon2 = { version = "0.6", default-features = false, features = ["alloc"] }`.

#### Does `cfb` do what we need?

- **Writes, not just reads.** `CompoundFile::create_with<F: Read + Write + Seek>` and
  `create_stream`/`create_storage`.
- **Works entirely in memory.** The generic API is `open_with<F: Read + Seek>`; the crate's own
  doc comment names `Cursor` and its tests use `Cursor::new(data)` / `Cursor::new(Vec::new())`.
  No `File` required — which is what makes it usable in wasm, where `std::fs` compiles but
  cannot work.
- **Has a strict parsing mode.** `OpenOptions::strict()` / `CompoundFile::open_strict`, whose
  doc says it "is stricter when parsing and will return an error if the file violates the spec
  in ways that are commonly seen in the wild". For untrusted input we want `open_strict`, and
  where it refuses a file Word accepts we want the *reason* reported rather than silently
  relaxing to permissive.
- **It does not have our limits.** `MAX_REGULAR_SECTOR` bounds exist and there is a
  `DEFAULT_STREAM_MAX_BUFFER_SIZE`, but there is no `PackageLimits` equivalent and no fuzz
  corpus of ours. So `casual-doc-cfb` is a **bounded wrapper** around `cfb`, in the same
  relationship `casual-doc-package` has to `zip`: the third-party crate does the format, our
  crate does the admission, the limits, the typed errors and the fuzz target. That is the
  existing, proven pattern in this repository and it is why `cfb` being third-party is the right
  answer rather than writing a CFB parser.

#### Alternatives considered and rejected

- **`ole` 0.1.15** — **WTFPL**, which is not in the nine permitted licences, so `cargo-deny`
  refuses it outright. Last released 2018-03-23. Unmaintained and unusable regardless of merit.
- **Writing our own AES/SHA/HMAC** — rejected, §9.2.

#### Prior art: four crates already attempt this, and none of them can be the dependency

Each was resolved against our own `deny.toml`, run through `cargo audit 0.22.2`, and **built for
`wasm32-unknown-unknown`**. Versions, licences and download counts re-checked against the
crates.io API.

| Crate | Version | Licence | `cargo audit --deny warnings` | wasm32 build | Scope |
| --- | --- | --- | --- | --- | --- |
| `office-crypto` | 0.4.0 (2026-09-13) | MIT | **FAILS** | builds | **decrypt only** |
| `ms-offcrypto-writer` | 1.0.7 (2025-09-05) | MIT OR Apache-2.0 | clean | builds | **encrypt only, agile only** |
| `msoffice-crypto` | 0.1.0-rc.5 (2026-09-30) | MIT OR Apache-2.0 | clean | **FAILS TO COMPILE** | detect + decrypt + encrypt, all schemes |
| `odf-crypto` | 0.1.0-rc.8 (2026-09-30) | MIT OR Apache-2.0 | clean | builds | ODF, not OOXML |

- **`office-crypto` 0.4.0** is the most used (196,889 downloads) and **fails our CI today**: it
  requires `quick-xml ^0.38.4`, which carries **two 7.5-high advisories** (RUSTSEC-2026-0195,
  unbounded namespace-declaration allocation; RUSTSEC-2026-0194, quadratic duplicate-attribute
  check), both needing ≥ 0.41.0 — and a `^0.38.4` requirement cannot be fixed with
  `cargo update`. It also pulls `derivative` 2.2.0 (RUSTSEC-2024-0388, **unmaintained, patched
  list empty**, so no fix will ever exist). Note the asymmetry our config produces: our
  `advisories.unmaintained = "workspace"` means `cargo-deny` would **not** flag the transitive
  `derivative`, but `cargo audit --deny warnings` does — so the PR job catches it and the weekly
  job would not. Decrypt-only in any case. **Unusable under our policy until upstream moves.**
- **`ms-offcrypto-writer` 1.0.7** passes every gate and builds for wasm, and is the most useful
  of the four **to read**: ~1,400 lines, deliberately "very simple and easy to audit", generic
  over `Read + Write + Seek` so it works on a `Cursor`. Its README states exactly the profile
  §2.6 arrives at independently (salt 16, AES-256, CBC, SHA-512, spin count 100000) and carries
  the HMAC-key-length deviation that became E12. Against it: write-only, agile-only, no
  configurability, **no zeroization of key material** (its own roadmap says "*maybe* implement
  zeroing of data structures"), 0 stars, one maintainer, 13 months without a commit, and 1.0.5
  and 1.0.6 are yanked.
- **`msoffice-crypto` 0.1.0-rc.5** claims the most complete coverage in Rust — agile and
  standard detect/decrypt/encrypt across AES-128/192/256 × SHA-1/256/384/512, plus RC4 CryptoAPI
  and XOR decrypt — and claims the two design properties §7.3 arrives at independently: the
  `dataIntegrity` HMAC verified over ciphertext **before** plaintext is returned, and
  `WrongPassword` / `IntegrityCheckFailed` / `UnsupportedAlgorithm` as distinct variants. That is
  the right shape, and it is corroboration worth having. But: **it does not compile for
  `wasm32-unknown-unknown`** — it passes `std::time::SystemTime` to
  `cfb::CompoundFile::set_modified_time`, whose wasm32 signature takes `web_time::SystemTime`
  (two `E0308`s). A two-line upstream fix, and proof **nobody has ever built it for wasm**. It is
  also pre-release with a self-admittedly unstable API, ~47,000 lines in a three-week-old
  repository, 0 stars, 99 downloads, and — the biggest supply-chain concern in this list — it
  **exact-pins a pre-release dependency from the same author**, `secure-gate = "=0.9.0-rc.12"`,
  in the key-material path.
- **`odf-crypto` 0.1.0-rc.8** is the same author with the same `secure-gate` pin, for ODF; its
  dependency set (`blowfish` + `cfb-mode`, `aes` + `cbc` + `aes-gcm`, `argon2` + `pbkdf2`) is the
  right shape for §3 and is useful confirmation of that shape.
- Also considered and out: **`msoffice_crypt`** — BSD-3-Clause FFI bindings to the C++
  `herumi/msoffice`, needs a C++ toolchain, **cannot target wasm**. **`aes-wasm`** — MIT, but
  `wasm32-wasi` only, so it cannot help a browser build. **`xlsx_encryptor`** — a wrapper, not a
  primitive.
- **Worth reading rather than depending on**, as reference implementations: `msoffcrypto-tool`
  (Python), `herumi/msoffice` (C++), and LibreOffice's `oox/source/crypto` and
  `package/source/zippackage`, which this document already quotes.

**Conclusion: implement on the primitives, and use `ms-offcrypto-writer` and `msoffice-crypto`
as reading.** Two independent reasons beyond the gate failures. First, no single crate covers
read + write + both schemes + wasm. Second, all four are built on the **previous** RustCrypto
generation (`aes 0.8` / `cbc 0.1` / `sha2 0.10` / `pbkdf2 0.12`) while §9.3 chooses the current
one (`aes 0.9` / `cbc 0.2` / `sha2 0.11` / `pbkdf2 0.13`) — which is where MSRV and security
attention goes. Pulling one in would put **two generations of the same cipher code in the wasm
bundle**; `multiple-versions = "warn"` makes that non-fatal and it is still a bundle we would be
shipping to a browser.

What to take from them regardless of the decision — the hard-won parts, not the code: the
HMAC-key-length deviation (E12), the salt-16/AES-256/SHA-512/spin-100000 tuple, and the
verify-HMAC-over-ciphertext-before-returning-plaintext ordering with a three-way error split.

One migration cost to expect, because it is the kind of thing that eats a day: examples and
reference code written against the old generation do not port mechanically. `rc4::Rc4` lost its
key-size generic; `hmac` moved `new_from_slice` from `Mac` to `KeyInit`; `cfb_mode::Encryptor` is
block-granular and the one you want is `BufEncryptor`; `rand_core`'s core trait is now
`Rng`/`TryRng`, not `RngCore`.

#### Audit status, stated with its real scope

`aes` says, in its own README:

> This crate has received one [security audit by NCC Group][6], with no significant findings. We
> would like to thank [MobileCoin][7] for funding the audit.

Link `[6]` is NCC Group's *MobileCoin RustCrypto AESGCM ChaCha20Poly1305 Implementation Review*,
2020-02-12. **That is the whole of the audit evidence found, and its scope is the AES-GCM and
ChaCha20Poly1305 implementations** (which exercise `aes`). Therefore, said plainly:

- **`aes`: covered**, by one 2020 review, through AES-GCM.
- **`cbc`, `cfb-mode`, `sha1`, `sha2`, `hmac`, `pbkdf2`, `blowfish`, `argon2`, `zeroize`, `cfb`:
  no third-party security audit was found.** They are widely used (`sha2` alone has over a
  billion downloads) and maintained by the RustCrypto organisation, and download counts are a
  maintenance signal, **not** an audit. Do not let this document be cited as saying they are
  audited.

Two warnings from `aes` that apply directly to us and should be recorded rather than paraphrased
away:

> This crate implements only the low-level block cipher function, and is intended for use for
> implementing higher-level constructions *only*. It is NOT intended for direct use in
> applications.

> This crate does not ensure ciphertexts are authentic (i.e. by using a MAC to verify ciphertext
> integrity), which can lead to serious vulnerabilities if used incorrectly! To avoid this, use
> an [AEAD] mode based on AES, such as AES-GCM …

We cannot take that advice: **MS-OFFCRYPTO specifies unauthenticated AES-CBC**, and an AEAD mode
would produce a file nothing else can open. The agile `dataIntegrity` HMAC is the format's own,
weaker answer (§8.2), and we must write it. This is an unavoidable property of interoperating
with a 2010 format, and it belongs in the threat model, not in a workaround.

#### The wasm AES answer, as far as it is sourced

`aes/src/lib.rs` selects its backend with a `cfg_if` cascade over
`aes_backend = "soft"`, then `target_arch = "x86_64" | "x86"` (AES-NI / VAES via `cpufeatures`),
then `target_arch = "aarch64"` (ARMv8 crypto extensions), then an `else` arm. **There is no
`wasm32` arm.** So a `wasm32-unknown-unknown` build takes the `else` branch and uses the
portable backend, which the crate documents as:

> a constant-time pure Rust implementation based on [fixslicing], a more advanced form of
> bitslicing implemented entirely in terms of bitwise arithmetic with no use of any lookup tables
> or data-dependent branches.

Constant-time, correct, and **software**. The crate even exposes
`aes::hardware_accelerated() -> bool` as a runtime check — which gives us an honest way to
report what a build actually got, and a guard that can assert it is `false` on wasm so nobody
later claims hardware AES in the browser.

**No published MB/s figure for AES-256-CBC in a browser wasm build was found.** That is
experiment **E8** (§7.5, §14) and it must run before phase 1 commits to this path. Saying
"roughly an order of magnitude slower than AES-NI" is the expected shape and is **not** a measured
number; it must not be published as one.


### 9.4 Rules that go with the dependencies

- **Vendor nothing.** `sources.unknown-registry = "deny"` and crates.io only.
- **`#![forbid(unsafe_code)]` on our crates**, as every existing crate does; the dependencies may
  use `unsafe` internally and that is their audited business, not ours.
- **No `getrandom` in the read path.** Decryption needs no randomness. This is what makes a
  read-only phase 1 cheap: it avoids the whole wasm-entropy question until we write.
- **Pin exact versions where the format depends on behaviour** (we already do this for `zip`:
  `= "=7.2.0"`), and let patch versions float for the primitives.
- **Known-answer tests, not just round trips.** A format implementation that only round-trips
  against itself is green and wrong — the exact `SKILL` §4 failure. Every scheme needs vectors
  from a file another product produced, committed as fixtures, with the expected plaintext
  digest. A round-trip-only test suite would pass with the byte order of `iterator` reversed.
- **The mutation rule applies** (`SKILL` §4): flip the `iterator` endianness, drop the
  `dataIntegrity` HMAC check, change one block-key byte, replace the constant-time compare with
  `==` — each must turn a test red, and the red output recorded.

---

## 10. Fixtures, and the problem that we have none

There is no encrypted document anywhere in `fixtures/`, and there cannot be a *useful* test
without one produced by another product. The corpus needed, before any implementation:

| Fixture | Produced by | Proves |
| --- | --- | --- |
| agile AES-256/SHA-512, `dataIntegrity` present | Word | the main read path |
| agile AES-128/SHA-1 | Word with the group policy set, or an older Office | the parameter space |
| agile, no `dataIntegrity` | hand-built from the spec | `minOccurs="0"` is honoured |
| standard AES-128/SHA-1/ECB, 50,000 iterations | LibreOffice (`Standard2007Engine`) | the legacy read path |
| ODF, Blowfish CFB-8, 1024 iterations | an old LibreOffice, or hand-built | the ODF 1.3 baseline |
| ODF, AES-256-CBC, PBKDF2 100,000 | current LibreOffice | the ODF modern path |
| ODF, Argon2id | current LibreOffice | that we refuse it with a *reason* |
| a `.doc` RC4 CryptoAPI file | LibreOffice | that a non-OOXML CFB is refused with a reason, not mis-parsed |
| a CFB that is not a document at all | hand-built | bounded refusal, no panic |
| adversarial CFB: FAT cycle, overlapping sectors, 2^31 directory entries, a `StreamSize` larger than the file | hand-built | the container's limits; **these belong in the fuzz corpus** |

Each fixture needs a committed expected-plaintext digest so a decryptor is checked against a
value, not against itself. **Sensitive documents stay local** — fixtures must be synthetic.
LibreOffice is available on this machine, which makes most of the table producible locally;
whether its CLI can be driven to *write* an encrypted file without a macro is experiment E9.

---

## 11. Phased implementation plan

Smallest shippable increment first. Each phase states what a user can do at the end of it.

### Phase 0 — Say the true thing. No cryptography at all.

The engine already knows (§1.2) and cannot say. Fix the detection contract.

- Add a CFB magic-byte sniffer (eight bytes, offset 0) and an "encrypted OOXML package" probe
  that looks for `\EncryptionInfo` and `\EncryptedPackage`. This needs a *minimal* CFB directory
  walk, which is the honest start of `casual-doc-cfb`.
- Add the "definite but locked" probe state and the typed `PasswordRequired` /
  `UnsupportedEncryption` refusals through `AdapterError` and `IoError` (§7.3).
- Make the existing ODF and ZIP encryption refusals reachable through auto-detection.
- Surface it in the chrome as a real message with a real reason, and emit a host `locked` event.
- Make `ImportRequest`/`ExportRequest` `#[non_exhaustive]` with builders, as a separable change
  (§7.3).
- Correct the `docs/135` line 313 sentence to match what a user experiences.

**At the end of it a user who opens a password-protected document is told that is what it is, and
which format, instead of "document format could not be detected".** A host is told the same
thing in a code it can branch on. This is a complete, shippable improvement with zero crypto
dependencies and zero security claims.

### Phase 1 — Read encrypted DOCX. The measurement comes first.

- Run **E8** (wasm AES/SHA throughput) and **E1** (Word/LibreOffice round-trip against real
  fixtures) **before** committing to the pure-Rust path. If E8 fails the time-to-interactive
  budget, D-5 is live and the design changes.
- Build the fixture corpus (§10).
- `casual-doc-cfb`: bounded MS-CFB **reader** — header, FAT, DIFAT, mini-FAT, directory,
  stream reads, with limits and a fuzz target. No crypto.
- `casual-doc-crypto`: MS-OFFCRYPTO agile **read** (the full §2.5 chain, `dataIntegrity`
  verified when present, constant-time verifier compare), then standard **read**.
- Password plumbed through `ImportRequest`; the Word/Docs-standard prompt in the chrome;
  `password-required` / `password-incorrect` refusal codes.
- Off the main thread, with progress and cancellation.

**At the end of it a user can open a password-protected `.docx` in the browser, with no server.
Saving writes an unencrypted file, and the UI says so at the moment of saving.** That sentence is
the honest version of the half-feature, and it is what ONLYOFFICE and Google Docs both do — the
difference is we say it before the user finds out.

### Phase 2 — Write encrypted DOCX, and preserve encryption on save.

- `casual-doc-cfb` gains a **writer** (the harder half: FAT allocation, mini-stream, directory).
- `casual-doc-crypto` gains agile **write** at the one profile in §2.6, `dataIntegrity`
  included, plus the `\0x06DataSpaces` streams.
- A CSPRNG appears for the first time: salts, the verifier input, the intermediate key, the HMAC
  key. **This is where a weak-randomness bug is catastrophic and silent**, and §9.3 answers the
  wasm entropy question: `casual-doc-crypto`'s writing functions **take** the random bytes rather
  than fetching them, the browser leaf supplies `crypto.getRandomValues` and a native host
  supplies `getrandom`. That is also the only shape in which a known-answer test of the writer is
  possible, because a test can supply a fixed vector.
- Round-trip gates: our file opens in Word and in LibreOffice; their files open in ours; the
  plaintext digest matches.
- "Keep the password on save" becomes the default for a file that arrived encrypted.

**At the end of it a user can set, change and remove a password on a `.docx`, and a file that
arrived encrypted stays encrypted when saved.** This is the thing ONLYOFFICE's web client cannot
do at all (§6.6).

### Phase 3 — ODF.

Read first (Blowfish CFB-8 and AES-256-CBC, PBKDF2; Argon2id refused with a reason), then write
the modern profile (§3.3). No CFB involved; it reuses `casual-doc-crypto` and adds `pbkdf2` and
`blowfish`. Resolve the ODF 1.4 `encrypted-package` question (E3) before writing.

**At the end of it `.odt` behaves like `.docx` did at the end of phase 2, and LibreOffice opens
what we write.**

### Phase 4 — PDF export with a password.

`/R 6` `/V 5` AES-256 only. Writer-only, so no PDF reader and no CFB. The honest claim is
narrower than for the Office formats (§4.3) and the UI copy must reflect that.

**At the end of it a user can export a PDF that asks for a password in Acrobat and in a
browser viewer.**

### Deliberately not in any phase

Certificate key encryptors, IRM/extensible encryption, ONLYOFFICE's "private rooms" wire format,
encrypted collaboration sessions, key escrow, `.doc`/`.xls` binary RC4, and writing anything a
standard annotates "MUST NOT" or "not recommended". Each would be a separate decision with its
own sourcing.

---

## 12. Decisions the owner must make

| | Decision | Options | Cost of each |
| --- | --- | --- | --- |
| **D-1** | **Which formats, in what order?** | (a) DOCX only; (b) DOCX → ODF → PDF; (c) PDF first because it is cheapest | (a) is the 90% case and the strongest competitive story (§6.6), and leaves `.odt` users with nothing. (b) is the complete answer and the longest. (c) ships fastest — no CFB at all — but delivers the weakest honest claim (§4.3) and does nothing for the format most people mean by "document encryption". **Recommendation: (b), with DOCX genuinely finished before ODF starts.** |
| **D-2** | **Read-only in phase one, or read + write?** | (a) read only, then write; (b) both together | (a) ships a real capability in roughly half the time, needs **no CSPRNG** and so dodges the wasm entropy question entirely, and is safe to claim. Its cost is a user experience that silently removes protection unless we say so loudly — which is the Google/ONLYOFFICE behaviour, and the thing their own dialog apologises for. (b) is coherent as a product but doubles phase 1 and puts a randomness bug on the critical path before any of the format work is proven. **Recommendation: (a), with the "this will save unencrypted" warning treated as a shipping requirement, not a nicety.** |
| **D-3** | **What does the product claim, in its own words?** | the owner's sentence | §8.3 is the permitted set and §8.4 the forbidden set. The sentence has to be written by the owner and then **guarded**, like every other number on `index.page.html` (`SKILL` §9.6). |
| **D-4** | **`spinCount` for files we write** | (a) 100000, matching Office and LibreOffice; (b) higher | (a) is interoperable by construction and is what every other product writes. (b) is cryptographically better and adds open latency linearly — at 1,000,000 the KDF is 10× and E8's number decides whether that is seconds. Note the spec ceiling is 10,000,000. **Recommendation: (a) now, revisit with E8's measurement.** |
| **D-5** | **Pure-Rust AES in wasm, or WebCrypto through the host?** | (a) `aes` crate everywhere; (b) WebCrypto in the browser, Rust elsewhere; (c) (a) now, (b) behind the host contract if E8 fails | (a) keeps one implementation and one behaviour across native/wasm/headless — the "prefer one mechanism over two" rule — at an order of magnitude in throughput. (b) is browser-fast and makes the engine depend on a host-provided primitive, which the host contract would have to specify and a headless host would have to supply. **The indicative measurement in §7.5 leans hard towards (a)**: CBC *decrypt* — our common case — runs at ~105 MiB/s in V8, so a 10 MB package is ~0.1 s, and WebCrypto would buy an order of magnitude on a cost that is already negligible. **Recommendation: (a), confirmed by E8's `crypto.subtle` control**, which gives the crossover size at which (b) would ever be worth a second cipher implementation. |
| **D-6** | **`.docm`, and encrypted macro-enabled files** | (a) keep refusing; (b) decide | `.docm` is already refused at open and `SKILL` §11 records that as undecided rather than an oversight. An encrypted `.docm` is the same question with the macro hidden until after decryption, which makes the refusal *harder*, not easier: we cannot know what is inside until we have the password. **Recommendation: keep refusing, and refuse after decryption with the real reason.** |
| **D-7** | **Does an encrypted document join a room?** | (a) no — encrypted implies standalone edit-and-save; (b) yes, with the plaintext relayed | The "one doc, one room" rule says a deployed or shared document always joins a room. An encrypted document in a room means the relay sees plaintext operations, which contradicts what a user will assume the password bought. (a) is honest and is a real product limitation. **Recommendation: (a), stated in the UI, until there is a design that does better.** |

---

## 13. Risks, worst first

| | Risk | Why it is the worst | What makes it not happen |
| --- | --- | --- | --- |
| **R1** | **We publish a security claim that is false.** | This repository has published fabricated claims twice (`SKILL` §9) and a wrong security claim is the one a user acts on by putting a real secret in a file. It is also unrecoverable reputationally for an Apache-2.0 project whose wedge is trust. | §8.3/§8.4 as a closed list; every published sentence tagged `data-claim` and re-derived by `tests/site_claims.test.mjs` like every other number; the matrix in §5 publishing its own holes rather than omitting them. |
| **R2** | **A silent cryptographic bug: we write files that look encrypted and are not, or that nothing can open.** | A wrong `iterator` byte order, a reused IV, a predictable "random" salt, or a CSPRNG that returns zeros in wasm all produce a file that *writes and reads back fine in our own tests* and is either unopenable elsewhere or trivially breakable. A round-trip test is green and wrong — `SKILL` §4's exact failure. | Known-answer vectors from files other products produced (§10), not round trips; cross-product round-trip gates in CI; the mutation rule applied to each constant and each byte order; and `getrandom` kept out of the tree until phase 2, where its wasm configuration gets its own guard. |
| **R3** | **Interoperability fails in one direction and we do not notice.** | LibreOffice's reader is a whitelist of four parameter combinations (§2.6) — AES-256 with SHA-256 is schema-legal and LibreOffice refuses it. A file Word opens and LibreOffice will not is a half-broken feature, and nothing in our test suite today would see it. | Write exactly one profile, chosen to be LibreOffice's own write preset; a CI gate that opens our output with a headless LibreOffice; fixtures from each producer in both directions. |
| **R4** | **The decrypted plaintext or the password leaks within our own process.** | The password arrives as a JavaScript string (unzeroable), the whole decrypted package sits in memory, and we embed in third-party pages. A host with a loose CSP turns our feature into an exfiltration channel. | A `SecretPassword` newtype that zeroizes and has an empty `Debug`; the password never in the DOM, never in a log, never in an error message; `AdapterError`'s existing "no document contents" contract extended to key material; a guard asserting no build posts a password anywhere; and the limits of all this written into §8.2 rather than engineered around. |
| **R5** | **wasm AES is too slow and we find out after building on it.** | *Downgraded from its original place by the §7.5 measurement.* The software backend is 25–50× off hardware, but CBC decrypt — the common case — measured ~105 MiB/s in V8, which is ~0.1 s for a 10 MB package, so the original fear does not hold on the read path. What remains: the **serial 100,000-iteration SHA-512 KDF is unmeasured** and cannot be chunked, and write throughput is 2.5–3× worse than read. | E8's narrowed form — a browser figure from a Worker, the `crypto.subtle` control, and **the KDF measured on its own** — before phase 1 commits; and the work is off-main-thread, progressive and cancellable from the first commit, which is the rule for anything O(document) anyway. |
| **R6** | **A hostile CFB file hangs or crashes the tab.** | A FAT is a linked list and a sector chain can be cyclic; a `StreamSize` can claim 2^63; a directory can be enormous. Our ZIP substrate is bounded and fuzzed precisely because this class is real, and a brand-new container starts with none of that. | `casual-doc-cfb` gets limits and a fuzz target in the same PR as its reader, not later; the adversarial fixtures in §10 are part of phase 0's definition of done; `forbid(unsafe_code)`. |
| **R7** | **Two branches collide on `ImportRequest`.** | Adding a field to a widely-constructed, non-`non_exhaustive` struct is the exact shape that made `main` red twice (`SKILL` §5a). | The `#[non_exhaustive]`-plus-builder change lands **first and alone**, in phase 0, before any crypto branch exists. |
| **R8** | **A new dependency fails a CI gate we did not think about.** | MSRV 1.88.0, the wasm target, nine permitted licences, `cargo audit --deny warnings`, `--locked` everywhere. A crate that is fine on crates.io can still fail four of those. | §9.3's table carries licence, MSRV and wasm status per crate; the dependency addition is its own commit so the gate failure is unambiguous; `cargo +1.96.0 fmt`, the full gate list, and `cargo check --workspace --all-targets` as always. |
| **R9** | **Scope creep into things that are not this.** | "Encryption" attracts certificate key encryptors, IRM, escrow, encrypted collaboration and digital signatures. Each is a separate programme and each would stall the one the owner asked for. | The "deliberately not in any phase" list in §11, and §12 D-7 answering the collaboration question up front rather than leaving it to be rediscovered. |

---

## 14. Open questions and the experiments that would settle them

Named, because `SKILL` §9 requires saying what is unsourced rather than smoothing it over.

| | Question | Experiment |
| --- | --- | --- |
| **E1** | Does Word open a file we write at the §2.6 profile, and does LibreOffice? And what parameters does Word *actually* write — the Office page contradicts itself on the hash (§2.6). | Produce password-protected `.docx` files with Word and with LibreOffice; parse their `\EncryptionInfo` and record `cipherAlgorithm`, `keyBits`, `hashAlgorithm`, `hashSize`, `spinCount`, `saltSize`, and whether `dataIntegrity` is present. Then write one ourselves and open it in both. |
| **E2** | Which cipher/KDF/start-key combination does each current LibreOffice version write by default for ODF, and at which ODF version setting? | Save the same `.odt` with a password from LibreOffice at each ODF version setting; read `META-INF/manifest.xml` and tabulate `algorithm-name`, `key-derivation-name`, `iteration-count`, `start-key-generation-name`, `checksum-type`. |
| **E3** | What exactly is ODF 1.4's `encrypted-package`, and is it in the published spec or a LibreOffice extension? | Read the ODF 1.4 part-2 text for `encrypted-package`; produce one from LibreOffice and inspect it. Our export already writes `manifest:version="1.4"`. |
| **E4** | Does the ONLYOFFICE *server* write an encrypted package in response to `"c": 'setpassword'`, or store the password out of band? | Clone `ONLYOFFICE/server`; grep for `setpassword` and for the `m_sPassword` plumbing into the x2t `TaskQueueDataConvert` params; and inspect `core`'s `X2tConverter`/`OfficeUtils` for a *write*-side consumer. |
| **E5** | Can Word open an encrypted `.odt` at all? | Produce one with LibreOffice; open it in Word; record the exact error. |
| **E6** | What does Google Docs do with an encrypted `.odt`? | Upload one to Drive; record whether it prompts, previews, or refuses. |
| **E7** | Does LibreOffice's PDF export really write `/R 6` AES-256, and at which version? | Export with a password from LibreOffice 25.8+; parse the `/Encrypt` dictionary for `/V`, `/R`, `/CF`. Replace the secondary source in §5.3 with this. |
| **E8** | Can a wasm build do AES-256-CBC and 100,000-iteration SHA-512 fast enough? | **Partly answered** — §7.5 has an indicative V8/Node figure (~41 MiB/s encrypt, ~105 MiB/s decrypt, ±2×) corroborated by one published Wasmtime figure. Still needed: a real browser figure from a dedicated Worker on an idle machine, the `crypto.subtle.decrypt('AES-CBC')` control in the same units, and **the serial SHA-512 KDF measured on its own** — the one cost the table does not cover and the one a user feels as a hung dialog. Still gates D-5 and any published number. |
| **E9** | Can the LibreOffice CLI be driven to *write* an encrypted file without a macro, so fixtures are reproducible in CI? | Try `--convert-to` with filter-data JSON for ODF and OOXML; if the password is a MediaDescriptor property rather than FilterData, fall back to a committed Basic macro or to generating fixtures from our own writer once phase 2 exists (and say so, because self-generated fixtures prove nothing — R2). |
| **E10** | Is `spinCount`'s `iterator` little-endian in the files Word writes? | Falls out of E1: decrypt a Word file with both byte orders and see which verifier round trip succeeds. Pin it as a known-answer test. |
| **E11** | What does ISO 32000 actually say about the standard security handler, Algorithm 2.A/2.B and the `/Encrypt` dictionary? | Obtain ISO 32000-2 (paywalled; it was **not** read for §4) and replace every secondary citation in §4 with the text. Phase 4 must not start before this. |
| **E12** | Is the agile `dataIntegrity` HMAC key `saltSize` bytes as §2.3.4.14 says, or **64** bytes as `ms-offcrypto-writer`'s README says Office's reference implementation uses? | Decrypt a Word-produced file and verify its `encryptedHmacValue` both ways; exactly one will check out. **Highest-value experiment before writing any agile file** (§2.5). |
| **E13** | Does `cfb` 0.15's wasm32 API really differ from its host API (`web_time::SystemTime` vs `std::time::SystemTime` on `set_modified_time`)? | It is what breaks `msoffice-crypto`'s wasm build (§9.3). Build a two-line probe against `cfb` 0.15 on both targets. If it holds, `casual-doc-cfb`'s wrapper must abstract it, and the `wasm` CI job is what would otherwise catch it late. |

### Proposed backlog row (not yet filed)

`docs/109` is the single queue and this document does not add to it; the row text is:

```text
| ENC-001 | Backlog | A password-protected .docx, .odt or encrypted ZIP opens as
"document format could not be detected" — the typed reason exists in three
crates and is unreachable because every `probe` collapses errors to `no_match` |
P2 | M | Open | New 2026-10-04 (docs/163 §1.2) | — | Measured through the
shipped wasm: `openAs` surfaces "encrypted ODF documents are unsupported" while
`open` says "could not be detected". docs/163 §11 Phase 0 is the fix (CFB magic
sniff, a locked probe state, typed PasswordRequired, a host `locked` event) and
carries no crypto dependency. Also corrects docs/135 line 313, which claims the
typed rejection is a product behaviour. |
```
