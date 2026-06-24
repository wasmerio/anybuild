from pathlib import Path

from shipit.builders.docker import DockerBuildBackend


class RecordingDockerBuildBackend(DockerBuildBackend):
    dockerfile_contents: str

    def build_dockerfile(self, image_name: str, contents: str) -> None:
        self.dockerfile_contents = contents


def test_base_packages_install_libsqlite3_before_mise(tmp_path: Path) -> None:
    backend = RecordingDockerBuildBackend(tmp_path, tmp_path / "assets")

    backend.build("test-image", {}, [], [])

    sqlite_index = backend.dockerfile_contents.index("libsqlite3-dev")
    mise_index = backend.dockerfile_contents.index(
        "RUN curl https://mise.run | sh"
    )
    assert sqlite_index < mise_index
