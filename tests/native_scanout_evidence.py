"""Analyze native P6 capture; nonuniform pixels do not certify Android Home."""
import base64
import collections
import hashlib
import struct
import zlib


def scanout_evidence(data):
    parts=data.split(b'\n',3)
    if len(parts)!=4 or parts[0]!=b'P6' or parts[2]!=b'255':raise ValueError('Expected native P6 capture')
    width,height=map(int,parts[1].split())
    pixels=parts[3]
    if not 1<=width<=4096 or not 1<=height<=4096 or len(pixels)!=width*height*3:raise ValueError('Invalid scanout size')
    counts=collections.Counter(pixels[i:i+3] for i in range(0,len(pixels),3))
    # A small exact nearest-neighbour preview for CI inspection without extra packages.
    w,h=min(160,width),min(120,height)
    raw=bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            offset=((y*height//h)*width+x*width//w)*3
            raw.extend(pixels[offset:offset+3])
    def chunk(kind,body):
        return struct.pack('>I',len(body))+kind+body+struct.pack('>I',zlib.crc32(kind+body))
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(bytes(raw)))+chunk(b'IEND',b'')
    return {'width':width,'height':height,'sha256':hashlib.sha256(pixels).hexdigest(),'unique_rgb_colors':len(counts),
            'dominant_color_fraction':counts.most_common(1)[0][1]/(width*height),
            'nonuniform':len(counts)>1,'android_home_verified':False,'preview_png_base64':base64.b64encode(png).decode()}
