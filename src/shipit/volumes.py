import json
import shutil
from pathlib import Path
from typing import Dict

from shipit.builders.base import BuildBackend
from shipit.shipit_types import Serve, Volume


def get_volumes_dir(src_dir: Path) -> Path:
    return src_dir / ".shipit" / "volumes"


def get_volume_mappings_path(src_dir: Path) -> Path:
    return get_volumes_dir(src_dir) / "mappings.json"


def build_volumes(src_dir: Path, serve: Serve) -> Dict[str, str]:
    volumes_dir = get_volumes_dir(src_dir)
    volumes_dir.mkdir(parents=True, exist_ok=True)

    mappings = {
        volume.name: str(volume.serve_path)
        for volume in serve.volumes or []
    }
    for volume in serve.volumes or []:
        volume.path.mkdir(parents=True, exist_ok=True)
        if _should_link_local_volume(src_dir, volume):
            _link_local_volume(volume)

    get_volume_mappings_path(src_dir).write_text(
        json.dumps(mappings, indent=2, sort_keys=True) + "\n"
    )
    return mappings


def load_volume_mappings(src_dir: Path) -> Dict[str, str]:
    mappings_path = get_volume_mappings_path(src_dir)
    if not mappings_path.is_file():
        return {}

    mappings = json.loads(mappings_path.read_text())
    if not isinstance(mappings, dict):
        raise ValueError("Volume mappings must be a dictionary")

    return {
        str(name): str(guest_path)
        for name, guest_path in mappings.items()
    }


def volume_mapdir_args(
    build_backend: BuildBackend,
    volume_mappings: Dict[str, str],
) -> list[str]:
    args: list[str] = []
    for name, guest_path in volume_mappings.items():
        host_path = build_backend.get_volume_path(name).absolute()
        host_path.mkdir(parents=True, exist_ok=True)
        args.append(f"--mapdir={guest_path}:{host_path}")
    return args


def _should_link_local_volume(src_dir: Path, volume: Volume) -> bool:
    shipit_dir = (src_dir / ".shipit").absolute()
    return volume.serve_path.is_absolute() and volume.serve_path.is_relative_to(
        shipit_dir
    )


def _link_local_volume(volume: Volume) -> None:
    source = volume.path.absolute()
    target = volume.serve_path

    if target.is_symlink():
        if target.resolve(strict=False) == source.resolve():
            return
        target.unlink()
    elif target.exists():
        if target.is_dir():
            if not any(source.iterdir()):
                shutil.copytree(target, source, dirs_exist_ok=True)
            shutil.rmtree(target)
        else:
            if not any(source.iterdir()):
                shutil.copy2(target, source / target.name)
            target.unlink()

    target.parent.mkdir(parents=True, exist_ok=True)
    target.symlink_to(source, target_is_directory=True)
