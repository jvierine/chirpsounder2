import os

import requests


DEFAULT_UPLOAD_URL = "https://juha.no/upload.php"


def post_to_server(fname, timeout=60, url=None):
    # edit /etc/apache2/conf-available/upload-limit.conf to enable station hosts
    if url is None:
        url = os.environ.get("IONOWEBSYNC_URL", DEFAULT_UPLOAD_URL)
    try:
        with open(fname, "rb") as f:
            response = requests.post(url, files={"file": f}, timeout=timeout)
        return response
    except Exception as exc:
        print("couldn't post file %s to server: %s" % (fname, exc))
        return None
