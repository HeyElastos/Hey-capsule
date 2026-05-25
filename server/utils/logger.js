const pino = require("pino");
const env = require("./env");

const transport = env.isProd
  ? undefined
  : {
      target: "pino-pretty",
      options: { colorize: true, translateTime: "SYS:HH:MM:ss" },
    };

const logger = pino({
  level: env.LOG_LEVEL,
  ...(transport ? { transport } : {}),
});

module.exports = logger;
