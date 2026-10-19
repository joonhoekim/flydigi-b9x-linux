# Linux (BlueZ) 메모

[English](linux-bluez.md)

BlueZ 5.82 에서 확인했다. 백엔드는 `crates/b9x-ble/src/bluez.rs`(bluer)다.

## 필수 설정: `LEAutoSecurity=false`

B9X 는 HID 서비스(`1812`)를 광고하지만 SMP 페어링 요청에 *Pairing Not Supported* 로 답한다.
BlueZ 의 HID 플러그인은 본딩 안 된 LE HID 기기에 연결하면 바로 페어링을 요청하고,
실패하면 연결 약 2초 만에 끊는다.

```ini
# /etc/bluetooth/input.conf
[General]
LEAutoSecurity=false
```

```sh
sudo systemctl restart bluetooth
```

`profiles/input/hog.c` 는 본딩 안 된 기기에만 이 값을 보므로, 이미 본딩된 키보드·마우스는 그대로 동작한다.

## 알아둘 동작

**펌웨어 버전.** B9X 는 알림을 켜자마자 버전(`30 2e 33 2e 30 00`, "0.3.0")을 보내는데,
CCCD 쓰기 응답보다 먼저 보낸다. bluetoothd 는 그 쓰기가 끝나기 전에 온 알림을 버려서 버전을 받지 못한다.
Device Information Service 에는 "Firmware Revision" 같은 항목 이름만 들어 있다. Linux 에서 `b9x info` 는 `?` 를 출력한다.

**연결이 프로세스보다 오래 남는다.** 프로세스가 연결을 끊지 않고 죽으면 BlueZ 가 LE 링크를 유지해 폰 앱이 붙지 못한다.
`b9x` 와 `b9x-gui` 는 Ctrl-C·SIGTERM·SIGHUP·stdout 닫힘에, 그리고 연결 도중 중단될 때도 연결을 끊는다.
`kill -9` 뒤에는 `b9x disconnect` 를 실행한다.

**연결 순서.** BlueZ 는 discovery 를 멈추면 페어링 안 된 기기를 지운다. 단 그 기기에 Connect 요청이 이미 왔으면 지우지 않는다.
반대로 커널은 LE 연결을 현재 스캔 주기(약 10초)가 끝날 때까지 미룬다.
그래서 광고를 본 뒤 Connect 를 보내고 바로 discovery 를 멈춘다.
BlueZ 안에서 연결이 멈춰 버리는 일도 있어 시도마다 제한 시간을 두고, 포기한 연결에는 Disconnect 를 보낸다(진행 중인 연결도 취소된다).

**쿨러 식별.** B9X 는 광고에 서비스 `ffe0`, scan response 에 이름 `Flydigi B9X` 를 싣는다.
`ffe0` 은 HM-10 같은 BLE 시리얼 모듈도 쓰므로 둘 다 맞아야 하고, `-a` 로 주소를 줘도 같다.
이미 연결된 기기도 다시 쓰기 전에 같은 방법으로 확인한다.

**쓰기 방식.** 앱은 write without response 를 쓰지만 특성 속성(`0x0a`)에 그 비트가 없어 기본은 write request 다.
`--write-cmd` 는 앱과 같게 보낸다. BlueZ 에서는 둘 다 동작한다.
