"""Linux only. Preview by default; --apply removes only safe Easy Switch leftovers."""
import argparse
import fcntl
import json
from pathlib import Path
import re


def plain(path):
    if path.is_symlink() or path.resolve() != path.absolute():
        raise ValueError(f"Refusing linked path: {path}")
    return path


def cleanup(scope, home, apply=False):
    plain(scope)
    target = plain(scope / "target.json")
    if Path(json.loads(target.read_text())["home"]) != home:
        return 0, 0, []
    # Use the same flock as the helper; never clean an active transaction.
    with plain(scope / "operation.lock").open("r+b") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        ops = plain(scope / "operations")
        if not ops.exists():
            return 0, 0, []
        count, freed, kept = 0, 0, []
        for directory in sorted(ops.iterdir()):
            plain(directory)
            if not directory.is_dir() or not re.fullmatch(r"\d{8}-\d{6}-[0-9a-f-]{36}", directory.name):
                continue
            manifest = plain(directory / "manifest.json")
            if not manifest.is_file():
                kept.append(f"{directory.name}: missing manifest")
                continue
            state = json.loads(manifest.read_text()).get("state")
            if state not in {"Preparing", "RolledBack", "Complete", "Restored"}:
                kept.append(f"{directory.name}: {state}; recovery files retained")
                continue
            files = list(directory.iterdir())
            for file in files:
                plain(file)
                if not file.is_file():
                    raise ValueError(f"Unexpected subdirectory: {file}")
            failed = state in {"Preparing", "RolledBack", "Restored"}
            selected = files if failed else [p for p in files if re.fullmatch(r"[0-9a-f-]{36}\.after", p.name)]
            # Leave the manifest until last so a full-disk interruption can be retried.
            selected.sort(key=lambda p: p.name == "manifest.json")
            size = sum(p.stat().st_size for p in selected)
            if apply:
                for file in selected:
                    file.unlink()
                if failed:
                    directory.rmdir()
            count += len(selected)
            freed += size
        return count, freed, kept


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    home = Path.home() / ".codex"
    total = 0
    for server in [".vscode-server", ".vscode-server-insiders"]:
        root = Path.home() / server / "data/User/globalStorage/easy-switch.easy-switch-remote"
        if not root.exists():
            continue
        plain(root)
        for target in sorted(root.glob("*/*/target.json")):
            count, size, kept = cleanup(target.parent, home, args.apply)
            print(json.dumps({"scope": str(target.parent), "apply": args.apply,
                              "files": count, "bytes": size, "retained": kept}))
            total += size
    print(json.dumps({"apply": args.apply, "totalMiB": round(total / 1048576, 2)}))


if __name__ == "__main__":
    main()
