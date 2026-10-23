"""Resume the pinned official release using validated HTTP range responses."""
import concurrent.futures, hashlib, pathlib, urllib.request, os

URL = 'https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_12.1.4_build/ghidra_12.1.4_PUBLIC_20260921.zip'
SIZE = 569649598
SHA256 = 'ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db'
ROOT = pathlib.Path(__file__).resolve().parents[1] / '.tools' / 'downloads'
DEST = ROOT / 'ghidra.zip'

def fetch(item):
    start, end = item
    part = ROOT / f'ghidra-{start}-{end}.part'
    if part.exists() and part.stat().st_size == end-start+1:
        return part
    offset = part.stat().st_size if part.exists() else 0
    req = urllib.request.Request(URL, headers={'Range': f'bytes={start+offset}-{end}'})
    with urllib.request.urlopen(req, timeout=90) as response:
        expected = f'bytes {start+offset}-{end}/{SIZE}'
        if response.status != 206 or response.headers.get('Content-Range') != expected:
            raise RuntimeError(f'Unverified range response: {response.status} {response.headers.get("Content-Range")}')
        with part.open('ab') as out:
            while data := response.read(1024*1024):
                out.write(data)
    if part.stat().st_size != end-start+1:
        raise RuntimeError('Incomplete range')
    print(f'Range {start}-{end} complete', flush=True)
    return part

if __name__ == '__main__':
    initial = DEST.stat().st_size if DEST.exists() else 0
    width = 32*1024*1024
    ranges = [(s, min(SIZE-1, s+width-1)) for s in range(initial,SIZE,width)]
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        parts = list(pool.map(fetch, ranges))
    with DEST.open('ab') as out:
        for p in parts:
            with p.open('rb') as src:
                while data := src.read(1024*1024):
                    out.write(data)
    digest = hashlib.file_digest(DEST.open('rb'), 'sha256').hexdigest()
    if DEST.stat().st_size != SIZE or digest != SHA256:
        raise RuntimeError(f'Release digest mismatch: {digest}')
    print(f'Verified official Ghidra release SHA256 {digest}', flush=True)
