# fetch-browser: Node.js + Playwright sidecar for browser automation.
# Skeleton — built out in M5.

FROM node:24-slim

RUN npx playwright install --with-deps chromium

WORKDIR /app

COPY package.json ./
RUN npm install --production 2>/dev/null || true

COPY . .

EXPOSE 3000

CMD ["node", "index.js"]
