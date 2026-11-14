"""HerKules Optional 1: USB-to-CAN bridge host."""

import argparse
import threading

import serial


def validate_tx(command: str) -> str:
    """Validate TX ID LEN DATA and return a complete line."""
    parts = command.strip().split()

    if len(parts) != 4 or parts[0].upper() != "TX":
        raise ValueError("Use: TX <hex_id> <length> <hex_data>")

    _, id_text, length_text, data_text = parts

    try:
        can_id = int(id_text, 16)
        length = int(length_text, 10)
    except ValueError:
        raise ValueError("Invalid CAN ID or length") from None

    if not 0 <= can_id <= 0x7FF:
        raise ValueError("Standard CAN ID must be 0x000-0x7FF")

    if not 0 <= length <= 8:
        raise ValueError("Classical CAN length must be 0-8")

    if length == 0:
        if data_text != "-":
            raise ValueError("Use '-' for zero-length data")
    else:
        if len(data_text) != length * 2:
            raise ValueError("Data length does not match LEN")
        try:
            bytes.fromhex(data_text)
        except ValueError:
            raise ValueError("Data must contain hex bytes") from None

    return f"TX {can_id:03X} {length} {data_text.upper()}\n"


def receive_loop(port, stop_event):
    """Continuously read responses and CAN notifications."""
    while not stop_event.is_set():
        try:
            line = port.readline()
            if line:
                message = line.decode(
                    "ascii", errors="replace"
                ).strip()
                print(f"\n[BOARD] {message}", flush=True)
        except serial.SerialException as exc:
            print(f"\n[ERROR] Serial disconnected: {exc}")
            stop_event.set()
            return


def main():
    parser = argparse.ArgumentParser(
        description="HerKules CAN-USB bridge"
    )
    parser.add_argument("port", help="Control COM port")
    args = parser.parse_args()

    try:
        port = serial.Serial(
            args.port,
            baudrate=115200,
            timeout=0.2,
            write_timeout=2,
        )
    except serial.SerialException as exc:
        print(f"Cannot open serial port: {exc}")
        return

    stop_event = threading.Event()
    reader = threading.Thread(
        target=receive_loop,
        args=(port, stop_event),
        daemon=True,
    )
    reader.start()

    print("HerKules CAN Bridge")
    print("Example: TX 123 4 01020304")
    print("Zero data: TX 123 0 -")
    print("Other commands: PING, SWD ID")
    print("Type quit to exit.")

    try:
        while not stop_event.is_set():
            command = input("> ").strip()

            if command.lower() in ("quit", "exit"):
                break

            if not command:
                continue

            if command.upper() in ("PING", "SWD ID"):
                message = command.upper() + "\n"
            else:
                try:
                    message = validate_tx(command)
                except ValueError as exc:
                    print(f"[INVALID] {exc}")
                    continue

            try:
                port.write(message.encode("ascii"))
            except serial.SerialException as exc:
                print(f"[ERROR] Send failed: {exc}")
                break

    except (KeyboardInterrupt, EOFError):
        pass
    finally:
        stop_event.set()
        port.close()
        reader.join(timeout=1)
        print("Bridge closed.")


if __name__ == "__main__":
    main()
