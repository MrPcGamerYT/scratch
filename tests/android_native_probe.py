"""Boot the prepared guest with no NIC/ADB and require actual virtio-agent replies."""
import argparse
import json
import hashlib
from pathlib import Path
import socket
import subprocess
import time
from android_boot_probe import capture_screen, PathEscape


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1',0)); return sock.getsockname()[1]


def run(args):
    output=Path(args.output).resolve(); output.mkdir(parents=True,exist_ok=True)
    result={'status':'failed','scope':'Linux TCG native virtio bridge; no NIC, ADB or Windows validation','adb_used':False}
    bridge_port,qmp_port=free_port(),free_port()
    command=['qemu-system-x86_64','-machine','q35','-accel','tcg,thread=multi','-cpu','max,avx=off,avx2=off,f16c=off,fma=off','-smp','2','-m','3072','-display','none','-nic','none','-vga','none','-device','virtio-vga','-chardev',f'socket,id=nexus_control,host=127.0.0.1,port={bridge_port},server=on,wait=off','-device','virtio-serial-pci,id=nexus_serial','-device','virtserialport,chardev=nexus_control,name=org.nexus.control.0','-qmp',f'tcp:127.0.0.1:{qmp_port},server=on,wait=off','-serial','stdio','-drive',f'file={PathEscape(args.disk)},format=qcow2','-drive',f'if=pflash,format=raw,readonly=on,file={PathEscape(args.uefi_code)}','-drive',f'if=pflash,format=raw,file={PathEscape(args.uefi_vars)}','-no-reboot']
    process=None; channel=None; started=time.monotonic(); buffer=b''; events=[]
    with (output/'serial.log').open('wb') as serial:
        try:
            process=subprocess.Popen(command,stdout=serial,stderr=serial)
            deadline=time.monotonic()+args.timeout; hello=False; phase='ready'; next_capture=0; captures=0; complete_replies=0; apk_data=b''; apk_offset=0
            while time.monotonic()<deadline:
                if time.monotonic()>=next_capture:
                    captures+=1
                    try: capture_screen(qmp_port,output/f'screen-{captures:02}.ppm')
                    except (OSError,RuntimeError,ValueError): pass
                    next_capture=time.monotonic()+60
                if process.poll() is not None: raise RuntimeError('Engine exited before bridge gates')
                if channel is None:
                    try: channel=socket.create_connection(('127.0.0.1',bridge_port),timeout=1); channel.settimeout(1)
                    except OSError: time.sleep(1); continue
                try: data=channel.recv(4096)
                except socket.timeout: continue
                if not data: raise RuntimeError('Native channel closed before gates')
                buffer+=data
                if len(buffer)>4096: raise RuntimeError('Oversized bridge response')
                while b'\n' in buffer:
                    line,buffer=buffer.split(b'\n',1)
                    text=line.decode('ascii'); events.append(text)
                    if line in (b'NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK',b'NEXUS 1 HELLO READY NAV VOLUME SHUTDOWN HOME_CHECK APP APK'): hello=True
                    elif line==b'NEXUS 1 READY 1' and hello and phase=='ready':
                        complete_replies+=1
                        if complete_replies>=3:
                            result['bridge_boot_ready_seconds']=round(time.monotonic()-started,2)
                            if args.provision:
                                channel.sendall(b'NEXUS 1 PROVISION_HOME\n'); phase='provision'
                            else:
                                channel.sendall(b'NEXUS 1 CHECK_HOME\n'); phase='check'
                    elif line==b'NEXUS 1 READY 0' and phase=='ready': complete_replies=0
                    elif line==b'NEXUS 1 RESULT PROVISION_HOME OK' and phase=='provision':
                        result['provisioned_this_boot']=True
                        channel.sendall(b'NEXUS 1 CHECK_HOME\n'); phase='check'
                    elif line==b'NEXUS 1 RESULT CHECK_HOME OK' and phase=='check':
                        result['home_without_adb']=True; result['no_recorded_anr']=True
                        result['home_verified_seconds']=round(time.monotonic()-started,2)
                        channel.sendall(b'NEXUS 1 QUERY_APP\n'); phase='app'
                    elif line.startswith(b'NEXUS 1 APP ') and phase=='app':
                        package=line[12:].decode()
                        if package not in ['com.android.launcher3','org.lineageos.trebuchet','com.bliss.launcher','com.farmerbb.taskbar']:
                            raise RuntimeError('Native app identity did not match resumed Home: '+package)
                        result['native_foreground_package']=package
                    elif line==b'NEXUS 1 RESULT QUERY_APP OK' and phase=='app':
                        if not result.get('native_foreground_package'): raise RuntimeError('No native foreground event')
                        channel.sendall(b'NEXUS 1 HOME\n'); phase='home'
                    elif line==b'NEXUS 1 APK NEXT' and phase=='apk':
                        if apk_offset<len(apk_data):
                            chunk=apk_data[apk_offset:apk_offset+65536]; apk_offset+=len(chunk)
                            channel.sendall(b'NEXUS 1 APK DATA '+str(len(chunk)).encode()+b'\n'+chunk)
                        else: channel.sendall(b'NEXUS 1 APK COMMIT\n'); phase='install'
                    elif line==b'NEXUS 1 RESULT APK_INSTALL OK' and phase=='install':
                        result['native_apk_install_completed']=True
                        channel.sendall(b'NEXUS 1 SHUTDOWN\n'); phase='shutdown'
                    elif line==b'NEXUS 1 RESULT HOME OK' and phase=='home':
                        result['native_home_command_completed']=True
                        result['home_command_completed_seconds']=round(time.monotonic()-started,2)
                        capture_screen(qmp_port,output/'screen-final.ppm')
                        if args.apk_fixture:
                            apk_data=Path(args.apk_fixture).read_bytes()
                            channel.sendall(b'NEXUS 1 APK BEGIN '+str(len(apk_data)).encode()+b' '+hashlib.sha256(apk_data).hexdigest().encode()+b'\n'); phase='apk'
                        else: channel.sendall(b'NEXUS 1 SHUTDOWN\n'); phase='shutdown'
                    elif 'FAILED' in text or 'ERROR' in text:
                        raise RuntimeError('Guest rejected native command: '+text)
                if phase=='shutdown':
                    process.wait(timeout=120)
                    if process.returncode: raise RuntimeError('Engine shutdown was abnormal')
                    result['clean_shutdown']=True; result['status']='native_bridge_home_shutdown_passed'; break
            else: raise RuntimeError('Timed out waiting for native boot/Home replies; inspect kernel, init policy and screenshots')
        except Exception as error:
            result['error']=str(error)
            if process and process.poll() is None:
                try: capture_screen(qmp_port,output/'screen-final.ppm')
                except Exception: pass
        finally:
            if channel: channel.close()
            if process and process.poll() is None:
                process.terminate()
                try: process.wait(timeout=15)
                except subprocess.TimeoutExpired: process.kill(); process.wait()
            result['elapsed_seconds']=round(time.monotonic()-started,2)
            (output/'bridge-events.txt').write_text('\n'.join(events))
            (output/'result.json').write_text(json.dumps(result,indent=2))
    return 0 if result['status']=='native_bridge_home_shutdown_passed' else 1


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    for name in ['disk','uefi-code','uefi-vars','output']: parser.add_argument('--'+name,required=True)
    parser.add_argument('--timeout',type=int,default=900)
    parser.add_argument('--apk-fixture',default='')
    parser.add_argument('--provision',action='store_true')
    raise SystemExit(run(parser.parse_args()))
