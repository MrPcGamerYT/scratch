//! Streaming SHA-256 for bridge file integrity; not APK signature verification.
use std::io::Read;
const K:[u32;64]=[
0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2];
fn block(h:&mut [u32;8],data:&[u8;64]) {
    let mut w=[0u32;64];for (n,bytes) in data.chunks_exact(4).enumerate() {w[n]=u32::from_be_bytes(bytes.try_into().unwrap());}
    for n in 16..64 {let a=w[n-15];let b=w[n-2];w[n]=w[n-16].wrapping_add(a.rotate_right(7)^a.rotate_right(18)^(a>>3)).wrapping_add(w[n-7]).wrapping_add(b.rotate_right(17)^b.rotate_right(19)^(b>>10));}
    let [mut a,mut b,mut c,mut d,mut e,mut f,mut g,mut hh]=*h;
    for n in 0..64 {
        let t=hh.wrapping_add(e.rotate_right(6)^e.rotate_right(11)^e.rotate_right(25)).wrapping_add((e&f)^(!e&g)).wrapping_add(K[n]).wrapping_add(w[n]);
        let u=(a.rotate_right(2)^a.rotate_right(13)^a.rotate_right(22)).wrapping_add((a&b)^(a&c)^(b&c));
        hh=g;g=f;f=e;e=d.wrapping_add(t);d=c;c=b;b=a;a=t.wrapping_add(u);
    }
    for (old,new) in h.iter_mut().zip([a,b,c,d,e,f,g,hh]) {*old=old.wrapping_add(new);}
}
pub fn digest(source:&mut dyn Read,limit:u64)->Result<(String,u64),String> {
    let mut h=[0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19];
    let mut buffer=[0u8;64];let mut used=0;let mut total=0u64;
    loop {
        let count=source.read(&mut buffer[used..]).map_err(|e|e.to_string())?;
        if count==0 {break;}total=total.checked_add(count as u64).ok_or("SHA length overflow")?;
        if total>limit||total>u64::MAX/8 {return Err("SHA input exceeds limit".into());}
        used+=count;if used==64 {block(&mut h,&buffer);used=0;}
    }
    buffer[used]=0x80;buffer[used+1..].fill(0);
    if used>=56 {block(&mut h,&buffer);buffer.fill(0);}
    buffer[56..].copy_from_slice(&(total*8).to_be_bytes());block(&mut h,&buffer);
    Ok((h.iter().map(|v|format!("{v:08x}")).collect(),total))
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn known_vectors_and_limit() {
        for (input,expected) in [(b"".as_slice(),"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),(b"abc","ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad") ,(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq","248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")] {
            assert_eq!(digest(&mut &input[..],1024).unwrap().0,expected);
        }
        assert_eq!(digest(&mut &vec![b'a';1_000_000][..],1_000_000).unwrap().0,"cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        assert!(digest(&mut &b"abc"[..],2).is_err());
    }
}
