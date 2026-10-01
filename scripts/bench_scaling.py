#!/usr/bin/env python3
"""Isolated synthetic source scaling; full runs and guard-only probes are distinct.
Default: index 10/100 MB, verify safe refusal at 1/10 GB without indexing them.
"""
import argparse, hashlib, json, os, platform, subprocess, tempfile, time
from datetime import datetime, timezone
from pathlib import Path

def generate(root, size, sparse=False):
    block_size=256_000
    for number, offset in enumerate(range(0, size, block_size)):
        count=min(block_size,size-offset)
        path=root/f'part_{number:06}.ts'
        if sparse:
            with path.open('wb') as file:
                file.write(b'// scan limit probe; sparse logical bytes\n')
                file.truncate(count)
        else:
            units=[]
            i=0
            while sum(map(len,units)) < count:
                body=''.join(f'  total += validateToken(token, {n}); // validate credential and accumulate result\n' for n in range(40))
                units.append(f'export function authenticateUser{i}(token: string): number {{\n  let total = 0;\n{body}  return total;\n}}\n')
                i+=1
            source=''.join(units)
            # Keep complete functions; pad the final file with a comment.
            source=source[:source.rfind('}\n',0,count)+2]
            padding=count-len(source)
            if padding>=4:source+='/*'+' '*(padding-4)+'*/'
            else:source+=' '*padding
            path.write_text(source)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',default='target/release/flexcontext-profile')
    parser.add_argument('--search-binary',default='target/release/flexcontext')
    parser.add_argument('--sizes',default='10000000,100000000,1000000000,10000000000')
    parser.add_argument('--output',required=True)
    parser.add_argument('--timeout',type=int,default=300)
    parser.add_argument('--resident-probe',action='store_true',help='Also profile a fresh resident process after populating the cache')
    args=parser.parse_args();rows=[]
    for size in map(int,args.sizes.split(',')):
        guard=size>256*1024*1024
        with tempfile.TemporaryDirectory(prefix='flexcontext-scale-') as directory:
            root=Path(directory);generate(root,size,sparse=guard)
            command=([args.search_binary,'search',str(root),'auth','--no-cache','--json'] if guard else [args.binary,str(root),'--query','authenticate token'])
            started=time.monotonic()
            try:
                result=subprocess.run(command,capture_output=True,text=True,timeout=args.timeout,env={**os.environ,'RAYON_NUM_THREADS':'4'})
                row={'source_bytes':size,'fixture':'sparse discovery probe' if guard else 'dense synthetic TypeScript','mode':'guard-only' if guard else 'full profile','elapsed_seconds':time.monotonic()-started,'exit_code':result.returncode}
                if result.returncode==0:
                    row['profile']=json.loads(result.stdout)
                    row['cache_bytes']=sum(path.stat().st_size for path in (root/'.flexcontext').glob('*.json'))
                    if args.resident_probe:
                        resident=subprocess.run([args.binary,str(root),'--query','authenticate token','--resident-only'],capture_output=True,text=True,timeout=args.timeout,check=True,env={**os.environ,'RAYON_NUM_THREADS':'4'})
                        row['fresh_resident_profile']=json.loads(resident.stdout)
                else:row['error']=result.stderr.strip()
                if guard and 'repository limit exceeded' not in result.stderr:raise RuntimeError(f'guard did not reject safely: {row}')
                if not guard and result.returncode:raise RuntimeError(row)
            except subprocess.TimeoutExpired:
                row={'source_bytes':size,'mode':'guard-only' if guard else 'full profile','error':'timeout','elapsed_seconds':args.timeout}
            rows.append(row)
            Path(args.output).write_text(json.dumps({'schema':1,'rayon_threads':4,'platform':platform.platform(),'measured_at':datetime.now(timezone.utc).isoformat(),'binary_sha256':hashlib.sha256(Path(args.binary).read_bytes()).hexdigest(),'runs':rows},indent=2)+'\n')
            print(f'{size}: {row["mode"]} {row["elapsed_seconds"]:.2f}s',flush=True)
if __name__=='__main__':main()
