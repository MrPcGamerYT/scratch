"""Native process invocation: avoid MSYS rewriting Windows SDK environment paths."""
import subprocess
import sys
from pathlib import Path

for label,arguments in [
    ('test',['test','--all-targets']),
    ('clippy',['clippy','--all-targets','--','-D','warnings']),
    ('release',['build','--release']),
]:
    command=['cargo',arguments[0],'--locked','--manifest-path','rust/nexus-engine/Cargo.toml',*arguments[1:]]
    p=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    print(p.stdout,flush=True)
    Path(f'native-engine-{label}.log').write_text(p.stdout,encoding='utf-8')
    if p.returncode:
        message=p.stdout[-3500:].replace('%','%25').replace('\r','%0D').replace('\n','%0A')
        print('::error::'+message,flush=True)
        sys.exit(p.returncode)
