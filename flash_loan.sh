set -e

export xrd=resource_sim1tknxxxxxxxxxradxrdxxxxxxxxx009923554798xxxxxxxxxakj8n3

echo "Resetting environment"
resim reset
export account=$(resim new-account | sed -nr "s/Account component address: ([[:alnum:]_]+)/\1/p")
echo "Account = " $account
echo "XRD = " $xrd

echo "Publishing dapp"
export flashloan_package=$(resim publish . | sed -nr "s/Success! New Package: ([[:alnum:]_]+)/\1/p")
echo "Package = " $flashloan_package

output=`resim call-function $flashloan_package FlashLoan instantiate_flash_loan $xrd $xrd 5 10 | awk '/Component: |Resource: / {print $NF}'`
export component=`echo $output | cut -d " " -f1`

export component_test=component_sim1cptxxxxxxxxxfaucetxxxxxxxxx000527798379xxxxxxxxxhkrefh

echo 'component = '$component

echo ' '
echo 'account = ' $account
echo 'xrd = ' $xrd
echo 'test faucet for lock fee = ' $component_test
echo ' '

echo '>>> Add Fund '

resim run resim/add_funds_100.rtm
# add_funds 100

resim run resim/add_funds_200.rtm
# add_funds 200

echo '>>> Partial withdraw '

resim run resim/partial_withdraw.rtm
# partial_withdraw 150

echo '>>> Show Account '
echo '>>> Account should show 9850 (10.000 - (300*50%))'
resim show $account

echo '>>> Show Component '
echo '>>> Component should show 150 (300*50%)'
resim show $component
