"""Challenge real terminal review I/O with fictional messages, never private mail."""
import errno
import json
import os
from pathlib import Path
import pty
import select
import signal
import sys
import tempfile
import time

def child():
    from evaluate import owner_review, ReviewProgress
    class Index:
        def query(self,*args): return [(1,.1),(2,.11),(3,.2),(4,.3)]
        def lexical(self,*args): return [2,1,4,3]
        def verify(self,*args): pass
    passages=[{'doc':{'id':mid,'sent':1734000000,'subject':'Fictional subject','text':'FICTIONAL FULL MESSAGE'},'text':'Fictional passage'} for mid in [10,10,20,30]]
    progress=ReviewProgress(Path(sys.argv[sys.argv.index('--progress')+1]),{'archive':'fictional','profile':'fixed','queries':'fixed'})
    try:
        result=owner_review(Index(),passages,[{'id':'fictional','text':'Fictional query'}],[None],progress)
    except EOFError:
        assert sum(len(v) for v in progress.data['grades'].values())==1
        print('SYNTHETIC_REVIEW_INTERRUPTED',flush=True)
        return
    for method in result[0]['methods'].values():
        assert method['returned']==method['judged']==3
        assert method['useful_count']==method['partial_count']==1
    print('SYNTHETIC_REVIEW_COMPLETE',flush=True)

def session(path,resume):
    pid,fd=pty.fork()
    if pid==0:
        os.execv(sys.executable,[sys.executable,str(Path(__file__).resolve()),'--child','--progress',str(path)])
    buffer=bytearray(); transcript=bytearray(); deadline=time.monotonic()+15
    def expect(pattern):
        while pattern not in buffer:
            remaining=deadline-time.monotonic()
            assert remaining>0,'Terminal prompt timed out'
            ready,_,_=select.select([fd],[],[],min(remaining,1))
            if not ready: continue
            try: data=os.read(fd,65536)
            except OSError as error:
                if error.errno==errno.EIO: data=b''
                else: raise
            assert data,'Terminal closed before expected prompt'
            buffer.extend(data); transcript.extend(data)
        end=buffer.index(pattern)+len(pattern); del buffer[:end]
    try:
        prompt=b'Your rating [2/1/0/?/f]: '
        expect(b'SEARCH 1 OF 1')
        if not resume:
            expect(prompt); os.write(fd,b'\n')
            expect(b'Please enter 2, 1, 0 or ?'); expect(prompt)
            os.write(fd,b'f\n'); expect(b'FULL MESSAGE'); expect(b'FICTIONAL FULL MESSAGE'); expect(prompt)
            os.write(fd,b'2\n'); expect(b'Rating saved.')
            data=json.loads(path.read_text()); assert sum(len(v) for v in data['grades'].values())==1
            expect(prompt); os.write(fd,b'\x04'); expect(b'SYNTHETIC_REVIEW_INTERRUPTED')
        else:
            expect(b'2 messages left to rate')
            expect(prompt); os.write(fd,b'0\n'); expect(b'Rating saved.')
            expect(prompt); os.write(fd,b'1\n'); expect(b'Rating saved.')
            expect(b'Search 1 complete.'); expect(b'SYNTHETIC_REVIEW_COMPLETE')
        _,status=os.waitpid(pid,0); pid=None
        assert os.waitstatus_to_exitcode(status)==0
    finally:
        os.close(fd)
        if pid is not None:
            os.kill(pid,signal.SIGKILL); os.waitpid(pid,0)

def main():
    from evaluate import ReviewProgress
    with tempfile.TemporaryDirectory(prefix='fictional-review-check-') as folder:
        path=Path(folder)/'ratings.json'
        session(path,False); session(path,True)
        data=json.loads(path.read_text()); assert sum(len(v) for v in data['grades'].values())==3
        try: ReviewProgress(path,{'archive':'different','profile':'fixed','queries':'fixed'})
        except AssertionError: pass
        else: raise AssertionError('Changed corpus reused prior ratings')
        assert not list(Path(folder).glob('*.tmp-*'))
    print(json.dumps({'real_controlling_terminal':True,'separate_input_output_streams':True,'numbered_search_banner':True,'blank_input_repeats_prompt':True,'full_message_expansion':True,'grade_saved_before_advancing':True,'interruption_and_resume':True,'mismatched_input_resume_refused':True,'grades_accepted':3,'aggregate_grading_completed':True,'private_correspondence_used':False}))

if __name__=='__main__':
    if '--child' in sys.argv: child()
    else: main()
