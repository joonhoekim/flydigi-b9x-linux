# 공식 앱 통신 캡처

[English](capturing.md)

[protocol.md](protocol.md) 의 프로토콜을 기록한 방법이다. 새 명령을 확인할 때 쓴다.
화이트리스트에는 이렇게 확인한 프레임만 넣고, opcode 추측·퍼징은 하지 않는다.
같은 제조사 펌웨어에 인증 없는 파괴적 opcode 가 있다:
BS3 Pro 의 `0xDF` 는 펌웨어 삭제, `0x06` 은 공장 초기화
([ElXreno/flydigictl](https://github.com/ElXreno/flydigictl) `docs/FIRMWARE.md` §3.3), B6 SDK 의 `FF 0A` 는 Flash 리셋이다.

## 준비

캡처용 기기는 root(KernelSU)된 Android 태블릿이다. HCI 로그를 읽으려면 root 가 필요하다.

```sh
adb devices -l                    # 유선
adb connect <tablet-ip>:5555      # 무선. KernelSU 부팅 스크립트가 포트를 5555 로 고정한다
```

전체 HCI snoop 로그를 켠다(재부팅 후에도 유지):

```sh
adb shell su -c 'setprop persist.bluetooth.btsnooplogmode full'
adb shell su -c 'settings put secure bluetooth_hci_log 1'
adb shell cmd bluetooth_manager disable
adb shell cmd bluetooth_manager enable       # 블루투스를 재시작해야 적용된다
adb shell su -c 'ls -la /data/misc/bluetooth/logs/'
```

`full` 이 아니면 `btsnooz_hci.log`(축약본)만 남아 페이로드를 볼 수 없다.

## 절차

1. (선택) logcat 을 받아둔다. 앱이 BLE 관련 로그를 남긴다:
   `adb logcat -v threadtime -T 1 > captures/logcat_$(date +%Y%m%d_%H%M%S).txt`
2. 앱을 열고 B9X 를 연결한다.
3. 조작은 **한 번에 하나씩, 간격을 두고** 하고 각 조작 시각을 적어둔다.
4. 로그를 가져와 파싱한다.
   ```sh
   tools/pull_snoop.sh [serial]
   python3 -I tools/btsnoop_att.py captures/btsnoop_….log [--addr AA:BB:…] [--all]
   ```
   기본 출력은 연결, GATT 표, write·notify 만 보여준다. `--all` 은 read, MTU 교환 등 나머지 ATT 도 보여준다.
5. 조작 시각과 write 를 맞추고 앱 화면과 대조해 `docs/protocol.md` 에 확정·추정을 구분해 적는다.
   프레임은 `testdata/app_writes.txt` 에 추가한다.

APK 정적 분석이 필요하면 BLE 코드는 `com.flydigi.sdk.bluetooth` 에 있다(B6-Studio 분석).

## 관련 구현

```sh
git clone https://github.com/shaoyou11/ha-flydigi-b8x refs/ha-flydigi-b8x   # B8X, Home Assistant
git clone https://github.com/pgs666/B6-Studio refs/B6-Studio                 # B6/B6X, APK 분석
git clone https://github.com/ElXreno/flydigictl refs/ElXreno-flydigictl      # BS 노트북 쿨러, 펌웨어
```

BS 시리즈는 HID 프레임(`cmd len payload`)이라 B9X 와 형식이 다르지만,
SoC 계열(WCH CH591/592 추정), 위험 opcode, `19 3x f0 f1 f2 f3 f4 + MAC` 식별자가 같다.
