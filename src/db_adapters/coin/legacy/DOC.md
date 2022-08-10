## Legacy contracts

The code here is duplicated more than it could be, but I decided not to generalize the logic because it's not intended to be reused or rewritten.

### Important details

#### Mint

Some contracts may mint coins at `new` method: see `tkn_near_events`.  
Some contracts may mint coins at `ft_on_transfer` method: see `wentokensir`.

General mint methods are `mint`, `near_deposit`. The logic inside could differ a little.

#### Transfer
All the legacy contracts have the same logic regarding TRANSFER except Aurora.  
They implement `ft_transfer`, `ft_transfer_call`, `ft_resolve_transfer`, the code for handling transfers is the same except Aurora.

#### Burn

Most of the contracts have burn logic: see `withdraw`, `near_withdraw`. The logic inside also differs a little.
