__all__ = ["version", "version_info"]


version = "0.21.4"  # x-release-please-version
_version_parts = version.split(".")
version_info = (
    int(_version_parts[0]),
    int(_version_parts[1]),
    int(_version_parts[2]),
    "final",
    0,
)
