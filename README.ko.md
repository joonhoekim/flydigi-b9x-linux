# flydigi-b9x-linux

**Flydigi B9X** 폰 쿨러를 공식 앱(飞智游戏厅, `com.game.motionelf`) 없이 Bluetooth LE 로 제어한다.
CLI 와 GTK 4 앱이 있고 Rust 로 작성했다.

[English](README.md)

> 비공식 프로젝트이며 Flydigi 와 관계가 없다. 프로토콜은 공식 앱의 통신을 캡처해 알아냈다.
> 사용에 따른 책임은 사용자에게 있다.

## 기능

- 실시간 상태: 냉면 온도, 팬 속도, 열면 온도, 냉각 출력
- 냉각 모드(Smart / Fixed / Custom), 프리셋, Custom 냉각 커브
- Quick switch(롱프레스 대상), OC Experience, 예약 종료
- 조명 켜기/끄기, LED 효과·속도·밝기·색
- 온도 그래프가 있는 GUI. 쿨러를 알아서 찾고, 여러 대면 고르게 한다

## 안전

일부 Flydigi 펌웨어는 모르는 opcode 를 받으면 자신을 지우거나 초기화한다.
이 프로젝트는 공식 앱이 보낸 것을 확인한 프레임만, 앱이 쓴 값 범위 안에서 보낸다.
모든 쓰기는 화이트리스트(`crates/b9x-proto`)를 거치고, 임의 프레임을 보내는 방법은 없다.
핸드셰이크 다음 쓰기는 기기가 B9X 로 광고하고 연결한 MAC 주소를 스스로 보고한 뒤에만 나간다.

## 플랫폼

| 플랫폼 | 상태 |
|---|---|
| Linux (BlueZ) | 지원: CLI, GTK 4 / libadwaita 앱 |
| macOS, Windows | Rust 판은 아직 없다. deprecated 된 Python CLI(`b9x/`)는 돈다 |

BLE 백엔드와 GUI 툴킷은 운영체제마다 따로 두어 각 플랫폼에 맞게 빌드한다.

## 설치 (Linux)

```sh
sudo apt install cargo libdbus-1-dev libgtk-4-dev libadwaita-1-dev pkg-config
cargo build --release          # target/release/b9x, target/release/b9x-gui
cargo install --path crates/b9x-cli && cargo install --path crates/b9x-gui   # 또는 ~/.cargo/bin 에 설치
```

B9X 는 HID 서비스를 광고하지만 페어링을 거부하고, BlueZ 의 HID 플러그인은 페어링에 실패하면 연결을 끊는다.
`/etc/bluetooth/input.conf` 에 아래를 한 번 넣고 `sudo systemctl restart bluetooth` 한다.
이미 본딩된 키보드·마우스에는 영향이 없다.

```ini
[General]
LEAutoSecurity=false
```

자세한 내용은 [docs/linux-bluez.ko.md](docs/linux-bluez.ko.md).

## 사용법

먼저 폰의 Flydigi 앱을 닫는다. 쿨러는 연결을 하나만 받는다.

```sh
b9x scan                                  # 쿨러 찾기
export B9X_ADDRESS=9C:7F:64:C5:F4:83      # 선택. 없으면 스캔한다
b9x status [--watch] [--json]
b9x mode smart --preset silent            # smart: max, silent / fixed: energy, strong, max
b9x mode custom --curve 5,5,5,6,7,8,8,8,8 # -12 … 12 °C 에서 레벨 1-8
b9x quick-switch fixed
b9x led breathing --speed fast --colors ff0000 0000ff --brightness 60
b9x light off
b9x shutdown 1h30m [--repeat]  |  b9x shutdown --off
b9x oc on --yes                           # 최고 출력, 과냉각 보호 해제. Ctrl-C 까지
b9x disconnect                            # 강제 종료된 b9x 가 남긴 연결 끊기
b9x --fake status                         # 모의 기기. 하드웨어 없이 시험
```

```sh
b9x-gui [-a ADDRESS] [--fake]
install -Dm644 data/io.github.flydigib9x.B9X.desktop ~/.local/share/applications/io.github.flydigib9x.B9X.desktop
```

## 제한

- Linux 에서는 펌웨어 버전을 보여주지 못한다. BlueZ 가 버전 push 를 버린다(BlueZ 문서 참고).
- 예약 종료는 255분까지다.
- OC Experience 는 연결돼 있는 동안만 유지된다. 연결이 끊기면 쿨러가 끝낸다.

## 문서

- [docs/development.ko.md](docs/development.ko.md): 코드 구성, 테스트, 동작 규칙
- [docs/linux-bluez.ko.md](docs/linux-bluez.ko.md): BlueZ 설정과 특이 동작
- [docs/protocol.md](docs/protocol.md): 프로토콜 분석 기록 (확정·추정 구분)
- [docs/capturing.ko.md](docs/capturing.ko.md): 공식 앱 통신 캡처

## 관련 프로젝트

- [shaoyou11/ha-flydigi-b8x](https://github.com/shaoyou11/ha-flydigi-b8x): B8X, Home Assistant 통합
- [pgs666/B6-Studio](https://github.com/pgs666/B6-Studio): B6/B6X Android 앱과 프로토콜 분석
- [ElXreno/flydigictl](https://github.com/ElXreno/flydigictl): BS 시리즈 노트북 쿨러, 펌웨어 분석

## 라이선스

[MIT](LICENSE)
