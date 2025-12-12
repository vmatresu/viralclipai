# ONNX Runtime Pre-built Artifacts

Pre-downloaded ONNX Runtime binaries for the `ort` crate. These are used instead of
the `download-binaries` feature for reproducible, offline-capable Docker builds.

## Version

**ONNX Runtime 1.22.0** - matches `ort-sys@2.0.0-rc.10` requirements.

## Files

| File | Target | SHA256 |
|------|--------|--------|
| `ort-linux-x64.tgz` | x86_64-unknown-linux-gnu | `ed1716de95974bf47ab0223ca33734a0b5a5d09a181225d0e8ed62d070aea893` |
| `ort-linux-aarch64.tgz` | aarch64-unknown-linux-gnu | `24e4760207136fc50b854bb5012ab81de6189039cf6d4fd3f5b8d3db7e929f1e` |

## Source

Downloaded from the `ort-rs` CDN (same source as `download-binaries` feature):
- https://cdn.pyke.io/0/pyke:ort-rs/ms@1.22.0/x86_64-unknown-linux-gnu.tgz
- https://cdn.pyke.io/0/pyke:ort-rs/ms@1.22.0/aarch64-unknown-linux-gnu.tgz

## Usage

The Dockerfiles extract these to `/usr/local/lib` and set `ORT_LIB_LOCATION=/usr/local/lib`
so the `ort-sys` build script finds them without downloading.

## Updating

When upgrading the `ort` crate version:
1. Check the new `dist.txt` in `~/.cargo/registry/src/*/ort-sys-x.x.x/dist.txt`
2. Download new artifacts for your target platforms
3. Verify SHA256 hashes match
4. Update this README with new version info
