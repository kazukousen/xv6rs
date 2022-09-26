import socket
import sys
import time

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
addr = ('0.0.0.0', int(sys.argv[1]))
buf = b'ping!'

while True:
    print("pinging...")
    sock.sendto(buf, addr)
    time.sleep(1)

