import re
import subprocess
import sys
from pathlib import Path

cargo_toml = Path("Cargo.toml").read_text(encoding="utf-8")
match = re.search(r'version\s*=\s*"(.*?)"', cargo_toml)
version = match.group(1)
tag = f"v{version}"

try:
    # pass
    subprocess.run(["git", "tag", tag], check=True)
    subprocess.run(["git", "push", "origin", tag], check=True)
except subprocess.CalledProcessError:
    print("Failed to create or push git tag")
    sys.exit(1)

print(f"{tag} pushed")
