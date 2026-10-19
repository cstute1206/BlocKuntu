"""Root-readable PID identities; proc-directory ownership is not the real UID."""
import argparse
import json
import os
from pathlib import Path


def collect_processes(proc=Path('/proc')):
    processes={}
    for entry in proc.iterdir():
        if not entry.name.isdigit():continue
        try:
            status=(entry/'status').read_text()
            uid=int(next(line.split()[1] for line in status.splitlines() if line.startswith('Uid:')))
            fields=(entry/'stat').read_text().rsplit(')',1)[1].split()
            processes[int(entry.name)]={'uid':uid,'exe':str((entry/'exe').readlink()),
                'ppid':int(fields[1]),'start_ticks':fields[19]}
        except (FileNotFoundError,ProcessLookupError,PermissionError,StopIteration,ValueError,IndexError):
            pass
    return processes


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--uid',type=int,required=True)
    args=parser.parse_args()
    assert os.geteuid()==0,'Reading non-dumpable session executables requires the root inventory helper'
    print(json.dumps({pid:row for pid,row in collect_processes().items() if row['uid']==args.uid}))
