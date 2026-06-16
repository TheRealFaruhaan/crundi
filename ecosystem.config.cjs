module.exports = {
  apps: [
    {
      name: 'crundi',
      script: 'src/index.js',
      interpreter: 'node',
      interpreter_args: '',

      // Restart on crash, but rate-limit restarts to avoid Telegram 429s.
      // Telegram rate-limits bot API calls dynamically — rapid restart loops
      // burn through limits fast (each restart = close() + start() = 2+ API calls).
      autorestart: true,
      watch: false,
      max_restarts: 10,        // max 10 restarts within min_uptime window
      min_uptime: '30s',       // runs < 30s count as crash restarts
      restart_delay: 30000,    // 30s between restarts (Telegram needs breathing room)

      // Give the process time to run service stop commands on shutdown
      kill_timeout: 5000,

      // Logs
      out_file: 'logs/out.log',
      error_file: 'logs/error.log',
      merge_logs: true,
      log_date_format: 'YYYY-MM-DD HH:mm:ss',

      // Pass shutdown signal so our handler fires
      shutdown_with_message: true,

      env: {
        NODE_ENV: 'production',
      },
    },
  ],
};
