"""Independent D3D11 shader/readback oracle. WARP is never hardware certification."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    exe=str(Path(sys.argv[1]).resolve())
    expected=bytes(v for y in range(64) for x in range(64) for v in (x%4*85,y%4*85,(x+y)%4*85,255))
    evidence={'schema':1,'guest_acceleration_verified':False,'android_graphics_verified':False,'samples':[]}
    report=Path('native-engine-graphics.json')
    try:
        with tempfile.TemporaryDirectory() as directory:
            for backend in ['hardware','warp']:
                for index in range(3):
                    path=Path(directory)/f'{backend}-{index}.rgba'
                    p=subprocess.run([exe,f'graphics-smoke-{backend}',str(path)],capture_output=True,text=True,timeout=30)
                    sample={'backend_requested':backend,'iteration':index+1,'exit':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
                    evidence['samples'].append(sample)
                    if p.returncode:
                        if backend=='hardware' and 'GRAPHICS_DEVICE_UNAVAILABLE backend=Hardware HRESULT=' in p.stderr:
                            sample['unavailable']=True
                            print('::notice::HARDWARE_GRAPHICS_UNAVAILABLE='+p.stderr.strip(),flush=True)
                            break
                        raise AssertionError(sample)
                    actual=path.read_bytes()
                    assert actual==expected,('Shader RGBA mismatch',sample,len(actual))
                    sample['exact_pixels']=True
                    decoded=json.loads(next(line.split('=',1)[1] for line in p.stdout.splitlines() if line.startswith('GRAPHICS_HOST_RENDER=')))
                    assert decoded['backend']==backend.capitalize()
                    assert decoded['guest_acceleration_verified'] is False and decoded['android_graphics_verified'] is False
                    original=actual
                    blocked=subprocess.run([exe,f'graphics-smoke-{backend}',str(path)],capture_output=True,text=True,timeout=30)
                    assert blocked.returncode!=0 and path.read_bytes()==original,'Existing render output was overwritten'
                    sample['existing_output_preserved']=True
        evidence['host_shader_validation_passed']=True
        evidence['hardware_renderer_verified']=any(s.get('exact_pixels') and s['backend_requested']=='hardware' for s in evidence['samples'])
        print('::notice::HOST_GRAPHICS_ACCEPTANCE='+json.dumps({key:value for key,value in evidence.items() if key!='samples'}),flush=True)
    finally:
        report.write_text(json.dumps(evidence,indent=2))


if __name__=='__main__':
    main()
