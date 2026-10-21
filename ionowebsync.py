"""Route scientific HDF5 products separately from images and station status."""
import os
import requests

DEFAULT_UPLOAD_URL = 'http://4.235.86.214/upload.php'

def post_to_server(fname, timeout=60, url=None):
    if url is None:
        url = os.environ.get('IONOWEBSYNC_URL', DEFAULT_UPLOAD_URL)
        if str(fname).lower().endswith('.h5'):
            url = os.environ.get('IONOWEBSYNC_H5_URL', url.rsplit('/', 1)[0] + '/upload_h5.php')
    try:
        with open(fname, 'rb') as handle:
            return requests.post(url, files={'file': handle}, timeout=timeout)
    except Exception as exc:
        print('could not post %s: %s' % (fname, exc), flush=True)
        return None
