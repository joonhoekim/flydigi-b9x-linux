# flydigi-b9x-linux

Flydigi B9X 폰 쿨러를 공식 앱(飞智游戏厅, `com.game.motionelf`) 없이
Linux·Windows·macOS에서 BLE로 제어하기 위한 프로토콜 분석 작업.

## 현재 상태

- B9X 전용 공개 구현은 없다.
- 형제 모델 구현을 출발점으로 쓴다.
  - [shaoyou11/ha-flydigi-b8x](https://github.com/shaoyou11/ha-flydigi-b8x) — B8X, Home Assistant 통합 (MIT).
    `custom_components/flydigi_b8x/protocol.py` 에 코덱이 있다.
  - [pgs666/B6-Studio](https://github.com/pgs666/B6-Studio) — B6/B6X, Android 앱 (Apache-2.0).
    `protocol-analysis.md` 에 명령 표와 상태 패킷 오프셋이 있다.
- 두 모델의 공통 GATT 채널:
  | 항목 | UUID |
  |---|---|
  | 서비스 | `0000ffe0-0000-1000-8000-00805f9b34fb` |
  | write | `0000ffe1-0000-1000-8000-00805f9b34fb` |
  | notify | `0000ffe4-0000-1000-8000-00805f9b34fb` |

  notify 구독 후 `13` 을 쓰면 20바이트 상태 프레임(첫 바이트 `0x13`)이 온다.
  설정 명령의 바이트 배치는 모델마다 다르다(B8X `20 …`, B6 `21/22/23 …`).
  B9X 가 어느 쪽인지, 혹은 다른 형식인지는 캡처로 확인한다.

## 안전 규칙

**공식 앱이 보내는 것을 캡처로 확인한 명령만, 앱이 쓴 값 범위 안에서만 보낸다.**
opcode 추측·퍼징은 하지 않는다.

같은 제조사 기기에서 인증 없이 받는 파괴적 opcode 가 확인돼 있다:

- BS3 Pro 쿨러 펌웨어: `0xDF` = 펌웨어 삭제 후 재부팅, `0x06` = 공장 초기화
  ([ElXreno/flydigictl `docs/FIRMWARE.md`](https://github.com/ElXreno/flydigictl) §3.3)
- B6 SDK: `FF 0A` = Flash 리셋 (B6-Studio `protocol-analysis.md`)

B9X 는 프레임 형식이 다르지만 같은 계열이라 미확인 opcode 는 위험한 것으로 취급한다.
모르는 필드는 앱 캡처·화면 대조·정적 분석으로 푼다.

## CLI

`b9x/` 는 [bleak](https://github.com/hbldh/bleak) 기반 CLI 다. Linux(BlueZ)·macOS·Windows 에서 돈다.

```sh
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/b9x scan                       # 주변 B9X 찾기
export B9X_ADDRESS=9C:7F:64:C5:F4:83     # macOS 는 scan 이 보여주는 UUID
b9x status [--watch] [--json]
b9x mode smart --preset silent           # smart: max·silent / fixed: energy·strong·max
b9x mode custom --curve 5,5,5,6,7,8,8,8,8
b9x quick-switch fixed                   # 롱프레스로 오갈 모드
b9x led breathing --speed fast --colors ff0000 0000ff --brightness 60
b9x light off
b9x shutdown 1h30m [--repeat] / b9x shutdown --off
b9x oc on --yes
```

기기는 연결을 하나만 받는다. 폰 앱이 백그라운드에서도 GATT 연결을 잡고 있으므로 앱을 강제 종료하거나 블루투스를 끈다.

- 모든 쓰기는 `b9x/protocol.py` 의 `check_frame()` 을 거친다.
  캡처에서 확인한 opcode·값 범위가 아니면 보내지 않고 `UnsafeFrame` 으로 멈춘다.
  임의 프레임을 보내는 옵션은 두지 않는다.
- 모드를 바꾸면 앱처럼 Quick switch 를 직전 모드로 맞춘다.
- 앱은 쓰기에 write without response 를 쓰지만, 특성 속성(`0x0a`)에 그 비트가 없어
  macOS·BlueZ 가 거부할 수 있다. 그래서 기본은 write request 다(macOS 에서 동작 확인). `--write-cmd` 로 앱과 같게 보낼 수 있다.
- Smart Control·Pulsating·Cyberpunk 는 앱이 설정 프레임을 보낸 적이 없어 선택만 된다.
- Gradient 는 8색 쓰기만 캡처했다. 2–8색을 허용하지만 8색 미만은 미검증이다.
- 예약 종료는 255분까지만 받는다. 분 값의 상위 바이트 순서를 확인하지 않았다.

테스트는 캡처한 앱 쓰기 전체를 화이트리스트에 대조한다.

```sh
.venv/bin/python -m unittest discover -s tests
```

## 디렉터리

| 경로 | 내용 |
|---|---|
| `docs/protocol.md` | B9X 프로토콜 분석 기록 (확정·추정 구분) |
| `b9x/` | CLI. `protocol.py` 가 코덱과 화이트리스트 |
| `tests/` | 캡처 프레임으로 코덱·화이트리스트 검증 |
| `tools/btsnoop_att.py` | btsnoop HCI 로그에서 LE 연결·특성 탐색·ATT write/notify 를 출력 (표준 라이브러리만 사용) |
| `tools/pull_snoop.sh` | 태블릿의 HCI 로그를 `captures/` 로 복사하고 바로 파싱 |
| `captures/` | 로그·APK 등 수집물. git 에 넣지 않는다 |
| `refs/` | 참고 레포 클론. git 에 넣지 않는다 — `git -C refs/<repo> pull` 로 갱신 |

## 형제 구현 받기

```sh
git clone https://github.com/shaoyou11/ha-flydigi-b8x refs/ha-flydigi-b8x     # B8X 폰 쿨러, HA 통합
git clone https://github.com/pgs666/B6-Studio refs/B6-Studio                   # B6/B6X 폰 쿨러, APK 분석
git clone https://github.com/ElXreno/flydigictl refs/ElXreno-flydigictl        # BS 노트북 쿨러, 펌웨어 역분석
git clone https://github.com/pipe01/flydigictl refs/pipe01-flydigictl          # 게임패드용. 쿨러와 무관
```

ElXreno/flydigictl 의 BS 시리즈는 HID 프레임(`cmd len payload`)이라 B9X 와 형식이 다르다.
다만 SoC(WCH CH591/592 계열 추정), 위험 opcode, `19 3x f0 f1 f2 f3 f4 + MAC` 식별자 같은 공통점이 있다.

## 분석 환경

캡처용 기기는 root(KernelSU)된 Android 태블릿이다.
HCI 로그를 읽으려면 root 가 필요하다.

### adb 연결

```sh
adb devices -l                         # 유선
adb connect <tablet-ip>:5555           # 무선
```

무선 adb 포트는 태블릿의 KernelSU 부팅 스크립트
`/data/adb/service.d/y705-adb-tcp.sh` 가 매 부팅 후 5555 로 고정한다.
태블릿 IP 는 DHCP 라 바뀔 수 있다 — 태블릿에서 `ip -4 addr show wlan0` 로 확인.

유선·무선이 동시에 붙어 있으면 `adb -s <serial>` 로 하나를 고른다.

### HCI snoop 로그 켜기

```sh
adb shell su -c 'setprop persist.bluetooth.btsnooplogmode full'
adb shell su -c 'settings put secure bluetooth_hci_log 1'
adb shell cmd bluetooth_manager disable
adb shell cmd bluetooth_manager enable     # 블루투스를 재시작해야 적용된다
adb shell su -c 'ls -la /data/misc/bluetooth/logs/'   # btsnoop_hci.log 가 생기면 성공
```

`persist.*` 라 재부팅 후에도 유지된다. 끄려면 모드를 `disabled` 로 바꾸고 블루투스를 재시작한다.
모드가 `full` 이 아니면 `btsnooz_hci.log`(축약본)만 남아 페이로드를 볼 수 없다.

## 캡처 절차

1. (선택) logcat 을 받아둔다. 앱이 BLE 관련 로그를 남기는 경우가 있다.
   ```sh
   adb logcat -v threadtime -T 1 > captures/logcat_$(date +%Y%m%d_%H%M%S).txt
   ```
2. 태블릿에서 飞智游戏厅 앱을 열고 B9X 를 연결한다.
3. 조작은 **한 번에 하나씩, 간격을 두고** 한다. 각 조작 시각을 대략 적어둔다.
   1. 조명 끄기 → 켜기
   2. 모드 전환 (자동·수동 등)
   3. 팬 속도 최소 → 최대
   4. 냉각 세기 최소 → 최대
4. 로그를 가져와 파싱한다.
   ```sh
   tools/pull_snoop.sh [serial]
   # 또는 저장된 로그를 다시 볼 때
   python3 -I tools/btsnoop_att.py captures/btsnoop_….log [--addr AA:BB:…] [--all]
   ```
   기본 출력은 연결/해제, 특성 목록, write·notify 만 보여준다. `--all` 은 read, MTU 교환 등 나머지 ATT 도 보여준다.
5. 조작 시각과 write 패킷을 맞춰보고, B8X·B6 코덱과 비교한다.

## 다음 단계

- 캡처로 B9X 의 명령·상태 프레임 형식을 확정한다.
- 필요하면 `captures/apk/base.apk` 를 정적 분석한다
  (B6-Studio 분석에 따르면 BLE 코드는 `com.flydigi.sdk.bluetooth`, 기기 프로토콜은 `com.flydigi.sdk.*` 패키지).
- 실기기에서 CLI 를 검증한다 (Linux·macOS).
