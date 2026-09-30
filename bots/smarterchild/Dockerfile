FROM python:3.12-slim
WORKDIR /app
COPY pyproject.toml ./
COPY smarterchild ./smarterchild
RUN pip install --no-cache-dir . && useradd -r -u 1000 bot && mkdir /data && chown bot /data
USER bot
ENV SMARTERCHILD_DATA=/data PYTHONUNBUFFERED=1
VOLUME /data
CMD ["smarterchild"]
