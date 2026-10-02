trigger RejectEmptyAccount on Account (before insert, before update) {
    for (Account record : Trigger.new) {
        if (record.Name == null || record.Name.trim().length() == 0) {
            record.addError('Account name is required');
        }
    }
}
