"""Basic static webserver for rendering"""

from flask import Flask, send_from_directory

app = Flask(__name__)


@app.route("/", defaults={'file': 'index.html'})
@app.route('/<path:file>')
def serve_results(file):
    """Send static files"""
    return send_from_directory("www", file)


if __name__ == "__main__":
    app.run()
