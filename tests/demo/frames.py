"""README screens from a real recorded run (tests/app-e2e.ps1 -Record <name>, the app window only):

    python -I tests/demo/frames.py <name>-app.mkv <theme> <t_start> <t_working> <t_approval> <t_done>

Writes docs/images/screen-{start,working,approval,done}-<theme>.png, the frames at those seconds.
"""
import os, subprocess, sys

mkv, theme, *ts = sys.argv[1:]
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'docs', 'images')
for name, t in zip(['start', 'working', 'approval', 'done'], ts):
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', '-ss', t, '-i', mkv, '-frames:v', '1', '-update', '1',
                    os.path.join(out, f'screen-{name}-{theme}.png')], check=True)
print('ok')
