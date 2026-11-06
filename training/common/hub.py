"""Pinned downloads from Hugging Face: a repository at a fixed revision, every file checked by SHA-256."""
import hashlib

from huggingface_hub import hf_hub_download


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch(repo: str, revision: str, filename: str, sha: str | None = None) -> str:
    """The local path of `filename` (Hugging Face's cache); refuses a file whose SHA-256 differs from `sha`."""
    path = hf_hub_download(repo_id=repo, filename=filename, revision=revision)
    if sha and sha256(path) != sha:
        raise RuntimeError(f"{repo}/{filename}@{revision}: SHA-256 {sha256(path)}, expected {sha}")
    return path
