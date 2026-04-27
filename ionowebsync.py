import requests


def post_to_server(fname, timeout=60):
    # edit /etc/apache2/conf-available/upload-limit.conf
    # to enable ip!
    url = "http://4.235.86.214/upload_h5.php"
    try:
        with open(fname, "rb") as f:
            response = requests.post(url, files={"file": f}, timeout=timeout)
        return response
    except Exception as exc:
        print("couldn't post file %s to server: %s" % (fname, exc))
        return None
