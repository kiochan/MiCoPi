# MiCoPi

[English](README.md) · [简体中文](README.zh.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

그리기 응답성을 우선하며, 확장 가능한 AI 연동 실행 코어를 갖춘 Rust 기반의 가벼운 네이티브 페인팅 앱입니다.

현재 **Windows 10/11 x64**를 지원합니다. 데스크톱 UI는 **egui + egui_dock**과 OpenGL을 사용하고, 펜 입력은 Windows Ink를 사용합니다. 아직 초기 개발 단계입니다.

**0.21.0:** [검증 가능한 이벤트 재생](docs/replay.md)을 추가했습니다. 필압, 레이어와 플러그인 작업을 기록하며 타임라인 재생, SHA-256 무결성 검증, 프로젝트 내 기록 저장, 기록 파일 및 PNG 프레임 내보내기를 지원합니다.

[원자적 작업 그룹](docs/atomic-operations.md)을 추가했습니다. 실패하면 문서 변경을 되돌리고 그룹 결과를 하나의 이벤트로 기록합니다. MCRPLY02는 메타데이터 차분, 타일 참조 및 XOR 압축을 사용하며 이전 형식도 읽을 수 있습니다.

![실제 MiCoPi 작업 화면. 스크린샷은 영어 UI입니다](docs/screenshots/workbench.png)

도구 모음과 캔버스는 독립적인 앱 내부 창으로, 플로팅, 네 방향 도킹 및 탭 그룹을 지원합니다. 기능 플러그인이 창 정의와 UI 소스를 소유하며 등록된 액션은 검색하고 단축키를 지정할 수 있습니다. [Plugin Windows](docs/plugin-windows.md)를 참고하세요.

브러시 패널에서 허용 오차를 지정하는 영역 채우기, 선형/방사형 그라데이션 드래그, 필압 설정과 곡선 미리 보기를 사용할 수 있습니다. [그리기 도구](docs/painting-tools.md)를 참고하세요.

## 핵심 철학

**그리기를 우선하고, 동작은 플러그인으로 구현하며, 사람과 자동화가 하나의 실행 코어를 사용합니다.**

펜 샘플은 타입이 지정된 인터페이스로 처리하여 필압, 손떨림 보정 및 부분 업데이트를 유지합니다. 플러그인, MCP와 headless는 스키마로 설명된 작업과 공개 상태를 통해 같은 Session을 조작합니다. 이미지 노드는 비파괴 합성을 제공하고, 작업 워크플로는 명시적으로 실행할 때만 동작합니다.

AI-ready는 기능을 검색하고 상태를 확인하며 실행 계획을 검사할 수 있는 연동 규약을 뜻합니다. 클라우드 연결이나 내장 AI 어시스턴트가 필수라는 의미는 아닙니다. 스크린샷이 포함된 영어 [설계 철학](docs/philosophy.md), [문서 목록](docs/README.md), [로드맵](ROADMAP.md), [디렉터리 구조](docs/project-structure.md)를 참고하세요. 기술 문서의 기준 언어는 영어입니다.

## 주요 기능

- 독립 DLL로 구현한 브러시, 지우개, 스포이트, 손 도구 및 확대/축소.
- Windows Ink 필압, 필압 곡선, 손떨림 보정 및 브러시 윤곽 커서.
- HSV 색상환, RGB 조절, 색상 견본 및 브러시 프리셋.
- 레이어 표시, 불투명도, 혼합 모드 및 타일 기반 실행 취소/다시 실행.
- 도킹, 부동 창, 탭 그룹을 지원하는 패널. 간격, 크기, 테마 및 바로 가기 키 설정.
- 검색 가능한 설정, JSON 편집, 명령 팔레트 및 바로 가기 키 기록.
- 투명 PNG 내보내기, PNG/JPEG/BMP 가져오기 및 `.mcp` 프로젝트.
- 레이어 연동 비파괴 이미지 그래프, 레이어별 필터, 자유 합성 및 플러그인이 등록하는 작업·상태 워크플로.
- JSONL 일괄 처리, MCP stdio 및 실행 중인 GUI 연결.

레이어 이름 변경, 복제(**Ctrl+J**), 순서 변경, 삭제와 정확한 뒤집기, 180° 회전, 정수 오프셋을 지원합니다. [레이어 편집](docs/layer-editing.md)을 참고하세요.

현재 Session당 문서 하나를 지원합니다. Wintab, 선택 영역, 자유 변형, 레이어 그룹/병합 및 수채화 엔진은 아직 지원하지 않습니다. 네이티브 DLL은 호스트와 같은 프로세스에서 실행됩니다.

## 빌드 및 실행

Rust 1.88 이상, Visual Studio C++ Build Tools 및 Windows SDK가 필요합니다.

```powershell
.\scripts\build.ps1 -PluginsOnly
cargo run --release --locked
.\scripts\build.ps1 -Test
```

`dist/MiCoPi.exe`를 실행하고, 같은 위치의 `plugins/`와 `licenses/`를 유지하세요. 최종 사용자는 Rust를 설치할 필요가 없습니다. 프로젝트 로컬 포터블 도구 모음은 영어 [개발 가이드](docs/development.md)를 참고하세요.

```powershell
New-Item -ItemType Directory -Force target/example-output | Out-Null
.\dist\micopi-cli.exe run examples/automation/draw.jsonl
.\dist\micopi-cli.exe actions
.\dist\micopi-cli.exe mcp
```

기본 MCP는 독립 캔버스를 사용합니다. `mcp --attach <profile>/session.json`으로 기존 GUI에 연결할 수 있습니다. 용도별 예제는 [examples](examples/README.md)에 있습니다.

## 표시 언어

기본 언어는 영어이며 중국어 간체, 일본어 및 한국어를 지원합니다. **Ctrl+,**로 설정을 열고 `ui.language`를 검색한 뒤 언어를 선택하고 저장하세요. 변경 사항은 즉시 미리 보기에 반영됩니다. 저장하지 않고 닫으면 마지막으로 저장한 설정으로 돌아갑니다.

```json
{"version":1,"ui":{"language":"ko"}}
```

언어를 바꿔도 작업 ID, 설정 키, 노드 포트 이름, 파일 경로 및 캔버스 픽셀은 바뀌지 않습니다. 자세한 내용은 영어 [현지화 가이드](docs/localization.md)를 참고하세요.

## 구조 및 기여

`src/`에는 호스트와 UI, `plugins/`에는 설치 가능한 패키지와 독립 알고리즘, `crates/plugin-api/`에는 ABI SDK가 있습니다. 문서, 스크린샷, 예제 및 개발 도구는 각각 `docs/`, `docs/screenshots/`, `examples/`, `scripts/`에 정리되어 있습니다. 빌드 결과, 배포 파일 및 개인 작품은 Git에 포함하지 않습니다.

영어 [기여 가이드](CONTRIBUTING.md), [아키텍처](docs/architecture.md) 및 [변경 기록](CHANGELOG.md)을 참고하세요. 영어 README를 기준으로 하며, 번역본도 같은 기능과 제약을 설명합니다.

## 라이선스

앱 소스는 [MIT](LICENSE)입니다. 내장 Google Material 아이콘은 [Apache 2.0](assets/material-symbols/LICENSE)을 따르며 [고지](assets/material-symbols/NOTICE.md)가 포함됩니다. 배포 파일에는 의존 라이브러리의 라이선스가 포함됩니다.
