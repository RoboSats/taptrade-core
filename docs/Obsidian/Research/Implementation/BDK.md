Loading from .env or environment:

```
use std::env;

dotenv::from_filename(".env")?;
let descriptor = env::var("DESCRIPTOR")?;
```

Creating/loading new Wallet:

```
use bdk::{Wallet, bitcoin::Network};


```