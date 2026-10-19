# 개발

[English](development.md)

## 구성

| 경로 | 내용 |
|---|---|
| `crates/b9x-proto` | 프레임 코덱과 화이트리스트. 통신 없음. 전송 계층은 검사된 `Frame` 만 받는다 |
| `crates/b9x-ble` | `Device`(연결 하나, 상태·응답 분배), `control`(앱과 같은 다단계 변경), 백엔드 `bluez`(Linux)·`fake`(모의 기기) |
| `crates/b9x-cli` | `b9x` |
| `crates/b9x-gui` | `b9x-gui`, GTK 4 / libadwaita. tokio worker 가 기기를 갖고, 창은 채널로 주고받는다 |
| `testdata/app_writes.txt` | 공식 앱이 보낸 쓰기 전체. Rust·Python 테스트가 같이 쓴다 |
| `docs/protocol.md` | 프로토콜 분석 기록 |
| `data/` | GUI 데스크톱 파일 |
| `tools/` | `btsnoop_att.py`(btsnoop 로그 ATT 파서), `pull_snoop.sh` |
| `b9x/`, `tests/` | deprecated 된 Python 구현 |
| `captures/`, `refs/` | 로컬 전용, git 에 넣지 않음: 캡처, 관련 프로젝트 클론 |

새 플랫폼은 공용 추상화 대신 `b9x-ble` 에 백엔드 모듈(`scan`·`find`·`connect`·`disconnect`)을 하나,
GUI crate 를 하나 추가한다.

## 테스트

```sh
cargo test
cargo clippy --all-targets
```

- 화이트리스트를 캡처한 앱 쓰기 전체와 거부해야 할 프레임 목록에 대조한다.
- `control` 흐름을 모의 기기로 돌린다. 실기기처럼 쓰기보다 늦게 따라오는 상태 push, 다른 기기의 핸드셰이크 응답도 포함한다.
- `b9x --fake …`, `b9x-gui --fake` 는 하드웨어 없이 돈다. `B9X_FAKE_DEVICES=3 b9x-gui --fake` 로 쿨러 선택 화면을 볼 수 있다.

## 동작 규칙

- 쓰기는 `Frame::check` 를 거친다. 여러 프레임 변경(LED 설정)은 첫 프레임을 보내기 전에 전부 검사한다.
- 모드를 바꾸면 앱처럼 Quick switch 를 직전 모드로 맞춘다.
- 상태 push 는 쓰기 뒤 1–2초 동안 옛 값을 보낸다. 쓰기 직후의 판단(직전 모드, Quick switch)은 방금 쓴 값을 기준으로 한다(최대 5초).
- Custom 커브 프레임(`20 04 …`)은 Custom 으로 전환도 하므로 Custom 모드에서만 보낸다.
- Smart Control·Pulsating·Cyberpunk 는 앱에도 설정이 없다. 앱처럼 선택만 한다.
- 쿨러는 연결이 끊기면 OC Experience 를 끝낸다. 그래서 `b9x oc on` 은 중단할 때까지 연결을 유지하고, 중단하면 OC 를 끈다.
  GUI 는 연결돼 있는 동안 유지한다.
- Gradient 는 8색 쓰기만 캡처했다. 2–8색을 허용하지만 8색 미만은 미검증이다.
- 예약 종료 분은 1바이트(최대 255)로 보낸다. 더 큰 값의 바이트 순서를 확인하지 않았다.
  폰 앱이 더 길게 설정한 예약을 끌 때는 255로 저장된다.

## Python (deprecated)

`b9x/` 는 [bleak](https://github.com/hbldh/bleak) 기반의 이전 CLI 다.
macOS·Windows 백엔드가 Rust 에 생길 때까지 참고용으로 둔다.
화이트리스트는 Rust 와 같아야 하고, 같은 `testdata/app_writes.txt` 로 테스트한다.

```sh
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/python -m unittest discover -s tests
```
