"""Make a small original Nexus animation; no upstream logos or proprietary assets."""
from pathlib import Path
import argparse
import io
import math
import zipfile
from PIL import Image, ImageDraw, ImageFont


def build(output):
    output = Path(output)
    if output.exists():
        raise FileExistsError(output)
    font = ImageFont.truetype('DejaVuSans.ttf', 40)
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_STORED) as archive:
        archive.writestr('desc.txt', '960 540 12\np 0 0 part0\n')
        for frame in range(24):
            image = Image.new('RGB', (960, 540), '#0b0d17')
            draw = ImageDraw.Draw(image)
            draw.rounded_rectangle((432, 152, 528, 248), 22, fill='#162844', outline='#63b7ff', width=3)
            draw.line([(459, 225), (459, 175), (501, 225), (501, 175)], fill='#e0efff', width=8)
            draw.text((480, 295), 'NEXUS', font=font, fill='#e0efff', anchor='mm')
            for dot in range(3):
                strength = int(100 + 130 * (1 + math.sin(frame * math.pi / 12 - dot)) / 2)
                x = 456 + dot * 24
                draw.ellipse((x - 4, 345, x + 4, 353), fill=(strength // 2, strength * 3 // 4, strength))
            stream = io.BytesIO(); image.save(stream, format='PNG')
            archive.writestr(f'part0/{frame:05}.png', stream.getvalue())


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('output')
    build(parser.parse_args().output)
