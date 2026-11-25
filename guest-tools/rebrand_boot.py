"""Replace only the verified Android-x86 text_art startup banner.
Copyright, licence, feature warnings, properties and package names stay intact.
"""
import gzip
import re
import stat
from install_bridge_ramdisk import records, record

BANNER=b'''\n    N   N  EEEEE  X   X  U   U  SSSS
    NN  N  E       X X   U   U  S
    N N N  EEEE     X    U   U   SSS
    N  NN  E       X X   U   U      S
    N   N  EEEEE  X   X   UUU   SSSS

===================================================
             Welcome to Nexus Android
==================================================='''


def patch_script(body):
    pattern=rb'(?m)(^text_art\(\)\n\{\ncat << "EOF"\n)(.*?)(\nEOF\n\})'
    matches=list(re.finditer(pattern,body,re.S))
    if len(matches)!=1: raise ValueError('Unrecognized init banner; nothing changed')
    match=matches[0]; original=match[2]
    if b'Welcome to Nexus Android' in original: return body
    if b'Have A Truly Blissful Experience' not in original or len(original)>4096 or any(x in original.lower() for x in (b'copyright',b'license',b'licence')):
        raise ValueError('Unrecognized banner/notice; nothing changed')
    return body[:match.start(2)]+BANNER+body[match.end(2):]


def patch(data):
    compressed=data.startswith(b'\x1f\x8b')
    raw=gzip.decompress(data) if compressed else data
    if len(raw)>64*1024**2: raise ValueError('Oversized ramdisk')
    output=bytearray(); found=False
    for name,fields,body in records(raw):
        if name.lstrip('./')=='init':
            if found or not stat.S_ISREG(fields[1]): raise ValueError('Duplicate/non-regular init')
            body=patch_script(body); found=True
        output+=record(name,fields,body)
    if not found: raise ValueError('No root init script')
    output+=record('TRAILER!!!',[0]*13,b'')
    output+=bytes((-len(output))%512)
    return gzip.compress(bytes(output),mtime=0) if compressed else bytes(output)
